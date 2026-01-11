use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use bytes::{Buf as _, BufMut as _, BytesMut};
use futures::{SinkExt, StreamExt as _};
use gradecope_proto::runner::{JobResponse, JobResult, Log, Switchboard as _};
use tarpc::{context::Context, server::Channel as _};
use tokio::task::JoinHandle;

use crate::ServerCtx;

pub struct Handle {
    join_handle: JoinHandle<eyre::Result<()>>,
}

#[derive(Clone)]
struct SwitchboardServer {
    server_ctx: Arc<ServerCtx>,
}

impl gradecope_proto::runner::Switchboard for SwitchboardServer {
    async fn request_job(self, _context: Context) -> JobResponse {
        tracing::debug!("SwitchboardServer::request_job called");
        JobResponse::Unavailable
    }

    async fn job_stopped(
        self,
        context: ::tarpc::context::Context,
        id: uuid::Uuid,
        result: JobResult,
        log: Log,
    ) -> () {
        todo!()
    }

    async fn request_cancellation_notifications(
        self,
        context: ::tarpc::context::Context,
        currently_running: Vec<uuid::Uuid>,
    ) -> Vec<uuid::Uuid> {
        todo!()
    }
}

/// Handles a websocket connection, including Ping/Pong heartbeats, and de/serializes messages for
/// a [`SwitchboardServer`] constructed from `server_ctx`.
#[tracing::instrument(skip(server_ctx, ws))]
async fn connected_runner(peer_addr: SocketAddr, server_ctx: Arc<ServerCtx>, mut ws: WebSocket) {
    tracing::info!("Runner connected from {peer_addr}");

    let switchboard_server = SwitchboardServer { server_ctx };

    let (bidi_a, mut bidi_b) = tarpc::transport::channel::bounded(16);

    let server = tarpc::server::BaseChannel::with_defaults(bidi_a);
    let jh = tokio::spawn(server.execute(switchboard_server.serve()).for_each(
        |response| async move {
            tokio::spawn(response);
        },
    ));

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
            msg = bidi_b.next() => {
                match msg {
                    Some(Ok(resp)) => {
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
            msg = ws.recv() => {
                match msg {
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Text(utf8_bytes) => {
                                let t = match serde_json::from_str(utf8_bytes.as_str()) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        tracing::warn!("Failed to deserialize message from runner: {e:?}");
                                        continue
                                    }
                                };
                                if let Err(e) = bidi_b.send(t).await {
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

    let _ = ws.close().await;
    if let Err(e) = jh.await {
        tracing::error!("Join error waiting for tarpc server: {e:?}")
    }
}

/// Upgrades a request to /runner/control to a websocket, and passes the websocket to
/// [`connected_runner`] if the upgrade was successful.
async fn websocket_route(
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<ServerCtx>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_failed_upgrade(move |e| {
        tracing::error!("Failed websocket upgrade on request from {peer_addr}: {e:?}");
    })
    .on_upgrade(move |ws| connected_runner(peer_addr, state, ws))
}

/// Spawns an Axum server that serves a single /runner/control route.
///
/// This runs unauthenticated HTTP, and MUST NOT exposed to the internet; a reverse proxy mTLS
/// termination MUST be used for production deployments.
pub async fn spawn_handler(server_ctx: Arc<ServerCtx>) -> eyre::Result<Handle> {
    let listener = tokio::net::TcpListener::bind(&server_ctx.opts.bind_server).await?;
    let router: axum::Router = axum::Router::new()
        .route("/runner/control", axum::routing::get(websocket_route))
        .with_state(server_ctx);
    let join_handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    });
    Ok(Handle { join_handle })
}
