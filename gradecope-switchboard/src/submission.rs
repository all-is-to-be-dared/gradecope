use std::{sync::Arc, time::Duration};

use bytes::{BufMut as _, BytesMut};
use futures::FutureExt as _;
use futures_concurrency::future::Race as _;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::UnixStream,
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};
use uuid::Uuid;

use crate::{ServerCtx, sql::SqlUser};

use gradecope_proto::ctl::{Request, Submission};

struct SubmissionListener {
    user: SqlUser,
    cancel_notifier: tokio::sync::oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}
pub struct SubmissionListenerSet {
    active_listeners: boxcar::Vec<SubmissionListener>,
}
impl SubmissionListenerSet {
    fn from_listeners(active_listeners: boxcar::Vec<SubmissionListener>) -> Self {
        Self { active_listeners }
    }
    pub async fn close(self) {
        for listener in self.active_listeners.into_iter() {
            let _ = listener.cancel_notifier.send(());
            let abort_handle = listener.join_handle.abort_handle();
            if let Err(_) =
                tokio::time::timeout(Duration::from_millis(10), listener.join_handle).await
            {
                abort_handle.abort();
            }
        }
    }
}

async fn accept_submission(
    server_ctx: &ServerCtx,
    user: &SqlUser,
    mut stream: UnixStream,
    submission: Submission,
) -> eyre::Result<()> {
    tracing::debug!("Received submission {submission:?}");

    let job_type_id = match sqlx::query!(
        "SELECT job_types.id FROM job_types WHERE spec = $1 LIMIT 1;",
        submission.spec
    )
    .fetch_one(&server_ctx.pool)
    .await
    {
        Ok(t) => t.id,
        Err(e) => {
            tracing::warn!(
                "In submission {submission:?}, no such job spec {spec}: {e:?}",
                spec = submission.spec
            );
            eyre::bail!("no such job spec");
        }
    };
    #[derive(sqlx::Type)]
    #[sqlx(type_name = "job_state", rename_all = "lowercase")]
    enum JobState {
        Submitted,
        Started,
        Canceled,
        Finished,
    }

    let job_id = Uuid::from_u128(rand::random());

    // TODO: JOB QUOTAS

    sqlx::query!(
        r#"
        INSERT INTO jobs (id, owner, job_type, commit, state, submit_timestamp)
        VALUES ($1, $2, $3, $4, $5, now());
        "#,
        job_id,
        user.id,
        job_type_id,
        submission.commit,
        JobState::Submitted as JobState,
    )
    .execute(&server_ctx.pool)
    .await?;
    let f = format!("> gradecope: Successfully started job \x1b[1;32m{job_id}\x1b[0m\r\n");
    let _ = stream.write_all(f.as_bytes()).await;

    Ok(())
}

async fn accept_request(
    server_ctx: &ServerCtx,
    user: &SqlUser,
    mut stream: UnixStream,
) -> eyre::Result<()> {
    let (buffer, mut stream) = timeout(Duration::from_millis(100), async move {
        let mut buf = BytesMut::with_capacity(1024);
        loop {
            match stream.read_buf(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "Failed to read from submission socket stream for user {user_name}: {e:?}",
                        user_name = &user.name
                    );
                    eyre::bail!("failed to read from stream: {e:?}");
                }
            }
            if !buf.has_remaining_mut() {
                tracing::error!("Submission socket stream yielding too many bytes, closing");
                eyre::bail!("received too many bytes from socket")
            }
        }
        Ok((buf.freeze(), stream))
    })
    .await
    // wrap timeout errors
    .inspect_err(|_| {
        tracing::error!(
            "Timed out while reading from submission socket stream for user {user_name}",
            user_name = &user.name
        );
    })
    .map_err(Into::into)
    // flatten timeout and interior errors and bubble
    .flatten()?;

    let request: Request = serde_json::from_slice(&buffer[..])
        .inspect_err(|e| tracing::error!("Error deserializing submission: {e:?}"))?;

    match request {
        Request::Submission(s) => accept_submission(server_ctx, user, stream, s).await,
    }
}

pub async fn spawn_socket_listeners(
    server_ctx: Arc<ServerCtx>,
) -> eyre::Result<SubmissionListenerSet> {
    let users = sqlx::query_as!(SqlUser, r#"SELECT * FROM "users";"#)
        .fetch_all(&server_ctx.pool)
        .await?;

    println!("{users:?}");

    let listeners = boxcar::vec![];
    for user in users {
        let user_homedir = server_ctx.opts.home_prefix.join(&user.name);
        if !user_homedir.is_dir() {
            tracing::error!(
                "Expected user {user_name} to have home directory {home_dir} but {home_dir} does not exist",
                user_name = user.name,
                home_dir = user_homedir.display()
            );
            eyre::bail!(
                "no such directory: {home_dir}",
                home_dir = user_homedir.display()
            );
        }
        let socket_path = user_homedir.join(&server_ctx.opts.submit_socket_path);
        if socket_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&socket_path).await {
                tracing::error!(
                    "Unable to remove old socket at {}: {e}",
                    socket_path.display()
                );
                Err(e)?;
            }
        }
        let socket_listener = match tokio::net::UnixListener::bind(&socket_path) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    "Failed to bind to socket {socket_path}: {e:?}",
                    socket_path = socket_path.display()
                );
                Err(e)?
            }
        };
        let (cancel_notifier, mut cancel_receiver) = oneshot::channel();
        let user2 = user.clone();
        let server_ctx2 = server_ctx.clone();
        let join_handle = tokio::spawn(async move {
            loop {
                enum Branch<T> {
                    Accept(T),
                    Cancel,
                }
                let raced = (
                    socket_listener.accept().map(Branch::Accept),
                    (&mut cancel_receiver).map(|_| Branch::Cancel),
                )
                    .race()
                    .await;
                match raced {
                    Branch::Accept(Ok((stream, _remote_addr))) => {
                        if let Err(e) = accept_request(&server_ctx2, &user2, stream).await {
                            tracing::warn!(
                                "Unable to process submission from socket {socket_path}: {e}",
                                socket_path = socket_path.display()
                            );
                        }
                    }
                    Branch::Accept(Err(e)) => {
                        tracing::error!(
                            "Failed to accept stream from socket {socket_path}: {e:?}",
                            socket_path = socket_path.display()
                        );
                    }
                    Branch::Cancel => break,
                }
            }
        });
        listeners.push(SubmissionListener {
            user,
            cancel_notifier,
            join_handle,
        });
    }

    Ok(SubmissionListenerSet::from_listeners(listeners))
}
