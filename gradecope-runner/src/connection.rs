use std::{
    cell::Cell,
    path::PathBuf,
    task::{Poll, Waker},
    time::Duration,
};

use bytes::{Buf as _, BufMut as _, BytesMut};
use futures::{SinkExt, Stream, StreamExt, stream::FuturesUnordered};
use gradecope_proto::runner::{
    JobResponse, SwitchboardClient, SwitchboardRequest, SwitchboardResponse,
};
use tarpc::{ClientMessage, Response, transport::channel::Channel};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};
use uuid::Uuid;

use crate::DeviceCtl;

pub async fn connect(opts: crate::Opts, devices: Vec<(Uuid, DeviceCtl)>) -> eyre::Result<()> {
    let req = format!("ws://{}/runner/control", opts.remote);
    let (stream, _response) = tokio_tungstenite::connect_async_with_config(&req, None, false)
        .await
        .inspect_err(|e| {
            tracing::error!("Failed to connect to `{req}`: {e:?}");
        })?;

    tracing::info!("Connect to remote at {}", opts.remote);
    let (client_channel, server_channel) = tarpc::transport::channel::bounded(16);
    let client = SwitchboardClient::new(tarpc::client::Config::default(), client_channel).spawn();

    tokio::spawn(server_proxy(stream, server_channel));
    dispatcher(
        client,
        devices,
        opts.test_runner,
        Duration::from_millis(opts.poll_interval_ms),
    )
    .await;

    Ok(())
}

type ServerChannel = Channel<ClientMessage<SwitchboardRequest>, Response<SwitchboardResponse>>;

#[pin_project::pin_project]
struct IncompleteFutures<T> {
    #[pin]
    inner: FuturesUnordered<T>,
    waker: Cell<Option<Waker>>,
}
impl<T> IncompleteFutures<T> {
    pub fn new() -> Self {
        Self {
            inner: FuturesUnordered::new(),
            waker: Cell::new(None),
        }
    }
    pub fn push(&self, fut: T) {
        self.inner.push(fut);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}
impl<T: Future> Stream for IncompleteFutures<T> {
    type Item = T::Output;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.inner.is_empty() {
            self.waker.set(Some(cx.waker().clone()));
            Poll::Pending
        } else {
            let this = self.project();
            this.inner.poll_next(cx)
        }
    }
}

async fn dispatcher(
    client: SwitchboardClient,
    mut devices: Vec<(Uuid, DeviceCtl)>,
    test_runner: PathBuf,
    poll_interval: Duration,
) {
    let mut assignments = vec![];
    let mut termination_receivers = IncompleteFutures::new();

    let mut poll_interval = tokio::time::interval(poll_interval);

    'outer: loop {
        tokio::select! {
        biased;
        msg = termination_receivers.next() => {
            match msg {
                Some(Ok(termination)) => {
                    if let Err(e) = client.job_stopped(tarpc::context::current(), termination).await {
                        tracing::error!("RPC error sending job termination status: {e:?}");
                    }
                }
                Some(Err(e)) => {
                    // wtf
                    tracing::error!("Failed to read receive termination: {e:?}");
                    return;
                }
                None => {
                    // wtf
                    tracing::error!("stopped");
                    return;
                }
            }
        }
        _ = poll_interval.tick() => {
            'assignments: while !devices.is_empty() {
                // tracing::debug!("Submitting job request");
                let job_spec = match client.request_job(tarpc::context::current()).await {
                    Ok(JobResponse::Job(job_spec)) => job_spec,
                    Ok(JobResponse::Unavailable) => break 'assignments,
                    Err(e) => {
                        tracing::error!("RPC error requesting job: {e:?}, killing dispatcher");
                        break 'outer;
                    }
                };
                let (worker_id, device) = devices.pop().unwrap();
                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
                let (return_tx, return_rx) = tokio::sync::oneshot::channel();
                let _handle = tokio::spawn(crate::runner::run_job(
                    worker_id,
                    device.clone(),
                    test_runner.clone(),
                    job_spec.clone(),
                    cancel_rx,
                    return_tx,
                ));
                termination_receivers.push(return_rx);
                assignments.push((job_spec, worker_id, device, cancel_tx));
            }

            match client
                .request_cancellation_notifications(
                    tarpc::context::current(),
                    assignments.iter().map(|assn| assn.0.id).collect(),
                )
                .await
            {
                Ok(job_ids) => {
                    'cancellations: for job_id in job_ids {
                        let Some(pos) = assignments.iter().position(|a| a.0.id == job_id) else {
                            continue 'cancellations;
                        };
                        let (_job_spec, worker_id, device, cancel_tx) = assignments.remove(pos);

                        if let Err(_) = cancel_tx.send(()) {
                            // receiver has been deallocated, no-op
                        }

                        devices.push((worker_id, device));
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "RPC error requesting cancellation notifications: {e:?}, killing dispatcher"
                    );
                    break 'outer;
                }
            };
        }
        }
    }
}

async fn server_proxy(
    mut ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    mut server_channel: ServerChannel,
) {
    let mut ping_interval = tokio::time::interval(Duration::from_secs(2));
    let mut ping_idx = 0;
    let mut tries = 0;
    const PING_LIMIT: u8 = 10;

    loop {
        tokio::select! {
            biased;
            _ = ping_interval.tick() => {
                tries += 1;
                if tries == PING_LIMIT {
                    tracing::error!("Runner stopped responding to heartbeat pings");
                    break;
                }
                let mut bytes = BytesMut::new();
                bytes.put_u64(ping_idx);
                if let Err(e) = ws.send(Message::Ping(bytes.freeze())).await {
                    tracing::error!("Failed to send Ping: {e:?}, closing socket");
                    break
                }
            }
            msg = server_channel.next() => {
                match msg {
                    Some(Ok(resp)) => {
                        // tracing::debug!("server_channel: >> {resp:?}");
                        let s = match serde_json::to_string(&resp) {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::error!("Failed to serialize message for runner: {e:?}");
                                continue
                            }
                        };
                        if let Err(e) = ws.send(Message::Text(s.into())).await {
                            tracing::error!("Failed to send Text with serialized RPC respones: {e:?}");
                            break
                        } else {
                            // tracing::debug!("Sent message!");
                        }
                    },
                    Some(Err(e)) => {
                        tracing::error!("Received error when pulling from internal channel: {e:?}");
                        break
                    },
                    None => {
                        break
                    },
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Frame(_) => {
                                unreachable!()
                            }
                            Message::Text(utf8_bytes) => {
                                let t = match serde_json::from_str(utf8_bytes.as_str()) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        tracing::warn!("Failed to deserialize message from runner: {e:?}");
                                        continue
                                    }
                                };
                                if let Err(e) = server_channel.send(t).await {
                                    tracing::error!("Dropping incoming message from runner: {e:?}");
                                }
                            },
                            Message::Binary(_bytes) => {
                                tracing::warn!("Received message of type Binary")
                            },
                            Message::Ping(_bytes) => {
                                if let Err(e) = ws.flush().await {
                                    tracing::error!("Failed to flush websocket for Pong: {e:?}");
                                }
                            },
                            Message::Pong(mut bytes) => {
                                let Some(pong_idx) = bytes.try_get_u64().ok() else {
                                    tracing::warn!("Malformed websocket Pong");
                                    continue
                                };
                                if pong_idx < ping_idx {
                                    if pong_idx != ping_idx - 1 {
                                        tracing::warn!("Received out-of-order Pong index: {pong_idx} when Ping index is {ping_idx}");
                                    }
                                    continue
                                } else if pong_idx == ping_idx {
                                    ping_idx += 1;
                                    tries = 0;
                                } else {
                                    tracing::warn!("Invalid websocket Pong index: {pong_idx} > {ping_idx}");
                                }
                            },
                            Message::Close(close_frame) => {
                                tracing::info!("Received close from runner: {close_frame:?}");
                                break
                            },
                        }
                    }
                    None => {
                        tracing::warn!("Runner disconnected");
                        break
                    },
                    Some(Err(e)) => {
                        tracing::warn!("Runner disconnected with error: {e:?}");
                        break
                    }
                }
            }
        }
    }
}
