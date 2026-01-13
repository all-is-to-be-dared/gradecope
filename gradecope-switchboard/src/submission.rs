use std::{sync::Arc, time::Duration};

use futures::FutureExt as _;
use futures_concurrency::future::Race as _;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _}, net::UnixStream, sync::oneshot, task::JoinHandle, time::timeout,
};
use uuid::Uuid;
use gradecope_proto::submit::Submission;
use crate::{ServerCtx, sql::SqlUser};
use crate::sql::JobState;

struct SubmissionListener {
    #[allow(unused)]
    user: SqlUser,
    cancel_notifier: oneshot::Sender<()>,
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

#[derive(Debug, thiserror::Error)]
enum SubmitError {
    #[error("Too many active jobs: {active} (user quota is {max})")]
    Quota {
        active: u32,
        max: u32,
    },
    #[error("Too many recent jobs: submitted {count} in the last hour (user quota is {max}")]
    TimeQuota {
        count: u32,
        max: u32,
    },
    #[error("Internal error")]
    Internal,
    #[error("Invalid job type: {spec}")]
    InvalidSpec { spec: String },
}

#[tracing::instrument(fields(user = %user), skip(user, stream, server_ctx))]
async fn accept_submission(
    server_ctx: &ServerCtx,
    user: &SqlUser,
    mut stream: UnixStream,
) -> eyre::Result<()> {
    let (stream, mut sink) = stream.split();
    let res : Result<Uuid, SubmitError> = try {
        const BUF_CAP: usize = 1024;
        let mut buffer = Vec::new();
        match timeout(Duration::from_millis(256),
                      stream.take(BUF_CAP as u64 + 1).read_to_end(&mut buffer)
        ).await {
            Ok(Ok(t)) => {
                if t > BUF_CAP {
                    tracing::error!("Failed to read from user {user_name}'s submission socket: input too long", user_name = &user.name);
                    eyre::bail!("input too long")
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Failed to read from user {user_name}'s submission socket: {e}", user_name = &user.name);
                Err(SubmitError::Internal)?
            }
            Err(_elapsed) => {
                tracing::error!(
                    "Timed out while reading from submission socket stream for user {user_name}",
                    user_name = &user.name
                );
                Err(SubmitError::Internal)?
            }
        }

        let submission: Submission = serde_json::from_slice(&buffer[..]).map_err(|e|  {
                tracing::error!("Error deserializing submission: {e}");
                SubmitError::Internal })?;

        if submission.user != user.name {
            tracing::error!(
                "Username mismatch: submission {submission:?} vs. socket for user {}",
                user.name
            );
            Err(SubmitError::Internal)?
        }

        tracing::debug!("Received submission {submission:?}");

        let job_type_id = match sqlx::query!(
            "SELECT job_types.id FROM job_types WHERE job_types.spec = $1 LIMIT 1;",
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
                Err(SubmitError::InvalidSpec { spec: submission.spec.clone() })?
            }
        };

        let active_jobs = match sqlx::query!(
            "SELECT COUNT(*) FROM jobs WHERE owner = $1 AND (state <> 'completed' OR state <> 'canceled')",
            user.id,
        ).fetch_one(&server_ctx.pool)
            .await {
            Ok(t) => {
                t.count.unwrap_or(0)
            }
            Err(e) => {
                tracing::error!("Failed to fetch active job count: {e}");
                Err(SubmitError::Internal)?
            }
        };
        if active_jobs < 0 {
            tracing::error!("Absurd value for active job count: {active_jobs}");
            Err(SubmitError::Internal)?
        }

        if active_jobs >= i64::from(server_ctx.opts.quota_max_concurrent_jobs) {
            tracing::debug!("User reached concurrent job count quota");
            Err(SubmitError::Quota { active: active_jobs.try_into().unwrap_or(u32::MAX), max: server_ctx.opts.quota_max_concurrent_jobs })?
        }

        let jobs_last_hour = match sqlx::query!(
            r#"SELECT COUNT(*) FROM "jobs" WHERE owner = $1 AND submit_timestamp >= NOW() - INTERVAL '1 HOUR';"#,
            user.id
        ).fetch_one(&server_ctx.pool).await {
            Ok(t) => t.count.unwrap_or(0),
            Err(e) => {
                tracing::error!("Failed to fetch number of submitted jobs in last hour: {e}");
                Err(SubmitError::Internal)?
            }
        };
        if jobs_last_hour >= i64::from(server_ctx.opts.quota_jobs_per_hr) {
            tracing::debug!("User reached per-hour job quota");
            Err(SubmitError::TimeQuota { count: jobs_last_hour.try_into().unwrap_or(u32::MAX), max: server_ctx.opts.quota_jobs_per_hr })?
        }

        let job_id = Uuid::from_u128(rand::random());

        match sqlx::query!(
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
            .await {
            Ok(_) => (),
            Err(e) => {
                tracing::error!("Failed to insert job: {e}");
                Err(SubmitError::Internal)?
            }
        }

        job_id
    };

    match res {
        Ok(job_id) => {
            let f = format!("> gradecope: Successfully started job \x1b[1;32m{job_id}\x1b[0m\r\n");
            let _ = sink.write_all(f.as_bytes()).await;
        }
        Err(e) => {
            let _ = sink.write_all(b"> gradecope: \x1b[1;31mFailed to submit job\x1b[0m.\r\n").await;
            let _ = sink.write_all(format!("> gradecope: Reason: {e}\r\n").as_bytes()).await;
        }
    }

    Ok(())
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
                tracing::error!("Unable to remove old socket at {}: {e}", socket_path.display());
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
                        if let Err(e) = accept_submission(&server_ctx2, &user2, stream).await {
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
