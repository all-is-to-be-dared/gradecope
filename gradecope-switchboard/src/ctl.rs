use std::sync::Arc;
use crate::{ServerCtx, sql::SqlUser};
use crate::sql::JobState;
use gradecope_proto::ctl::{Ctl, CtlError, JobReference, JobResult, JobStatus, Log};
use tarpc::{
    context,
    serde_transport::unix,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Json,
};
use futures::StreamExt;
use tokio::net::unix::UCred;
use users::get_user_by_uid;
use eyre::OptionExt;

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

impl From<JobState> for JobResult {
    fn from(state: JobState) -> Self {
        match state {
            JobState::Submitted => JobResult::Pending,
            JobState::Started => JobResult::Running,
            JobState::Canceled => JobResult::Canceled,
            JobState::Completed => JobResult::Completed,
            JobState::Error => JobResult::Error,
            JobState::Timeout => JobResult::Timeout,
        }
    }
}

/// PER CONNECTION state
#[derive(Clone)]
struct CtlService {
    credentials: UCred,
    server_ctx: Arc<ServerCtx>
}

impl CtlService {
    async fn user(&self) -> eyre::Result<SqlUser> {
	let user = get_user_by_uid(self.credentials.uid()).ok_or_eyre("couldn't get ID")?;
	let username = user.name().to_str().ok_or_eyre("couldn't convert name to &str")?;
	match sqlx::query_as!(
	    SqlUser,
	    "SELECT * FROM users WHERE name = $1 LIMIT 1;",
	    username
)
	.fetch_one(&self.server_ctx.pool)
	.await
	{
	    Ok(t) => Ok(t),
	    Err(_) => {
		tracing::warn!(
		    "No such user {username:?}",
		);
		eyre::bail!(CtlError::NotFound(username.to_string()));
	    }
	}
    }
    #[allow(dead_code)]
    fn check_admin(&self) -> eyre::Result<()> {
	// TODO
	eyre::bail!(CtlError::PermissionDenied);
    }

    #[tracing::instrument(skip(self))]
    async fn accept_submission(&self, commit: String, job_spec: String) -> eyre::Result<()> {
	tracing::debug!("WIP");
	eyre::bail!(CtlError::PermissionDenied);
    }

    #[tracing::instrument(skip(self))]
    async fn get_status(&self, job: JobReference) -> eyre::Result<JobStatus> {
	let user = self.user().await?;

	let row = sqlx::query!(
	    r#"
	    SELECT jobs.id, job_types.spec, jobs.state as "state: JobState"
	    FROM jobs
	    JOIN job_types ON jobs.job_type = job_types.id
	    WHERE jobs.owner = $1 AND jobs.id = $2 AND job_types.spec = $3
	    LIMIT 1;
	    "#,
	    user.id,
	    job.job_id,
	    job.job_spec
	)
	.fetch_optional(&self.server_ctx.pool)
	.await
	.map_err(|e| {
	    tracing::error!("Failed to fetch job status: {e}");
	    eyre::eyre!(CtlError::InternalError(e.to_string()))
	})?;

	match row {
	    Some(row) => Ok(JobStatus {
		job_spec: row.spec,
		job_id: row.id,
		result: row.state.into(),
	    }),
	    None => eyre::bail!(CtlError::NotFound(format!(
		"Job {} with spec {} not found",
		job.job_id, job.job_spec
	    ))),
	}
    }

    #[tracing::instrument(skip(self))]
    async fn get_history(&self, job_spec: Option<String>) -> eyre::Result<Vec<JobStatus>> {
	let user = self.user().await?;

	let jobs: Vec<JobStatus> = match job_spec {
	    Some(spec) => {
		// Return all jobs for the given job spec
		let rows = sqlx::query!(
		    r#"
		    SELECT jobs.id, job_types.spec, jobs.state as "state: JobState"
		    FROM jobs
		    JOIN job_types ON jobs.job_type = job_types.id
		    WHERE jobs.owner = $1 AND job_types.spec = $2
		    ORDER BY jobs.submit_timestamp DESC;
		    "#,
		    user.id,
		    spec
		)
		.fetch_all(&self.server_ctx.pool)
		.await
		.map_err(|e| {
		    tracing::error!("Failed to fetch job history: {e}");
		    eyre::eyre!(CtlError::InternalError(e.to_string()))
		})?;

		rows.into_iter()
		    .map(|row| JobStatus {
			job_spec: row.spec,
			job_id: row.id,
			result: row.state.into(),
		    })
		    .collect()
	    }
	    None => {
		// Return most recent job for each job spec
		let rows = sqlx::query!(
		    r#"
		    SELECT DISTINCT ON (job_types.spec)
			jobs.id, job_types.spec, jobs.state as "state: JobState"
		    FROM jobs
		    JOIN job_types ON jobs.job_type = job_types.id
		    WHERE jobs.owner = $1
		    ORDER BY job_types.spec, jobs.submit_timestamp DESC;
		    "#,
		    user.id
		)
		.fetch_all(&self.server_ctx.pool)
		.await
		.map_err(|e| {
		    tracing::error!("Failed to fetch job history: {e}");
		    eyre::eyre!(CtlError::InternalError(e.to_string()))
		})?;

		rows.into_iter()
		    .map(|row| JobStatus {
			job_spec: row.spec,
			job_id: row.id,
			result: row.state.into(),
		    })
		    .collect()
	    }
	};

	Ok(jobs)
    }
}

impl Ctl for CtlService {
    async fn hi(self, _: context::Context) -> String {
	let name = match self.user().await {
	    Ok(u) => u.name,
	    Err(e) => {
		tracing::error!("Failed to fetch user: {e}");
		"no name".to_owned()
	    }
	};
	return format!("Hello, {}!", name);
    }

    async fn submit(self, _: context::Context, commit: String, job_spec: String) -> Result<(), CtlError> {
	return self.accept_submission(commit, job_spec).await
	    .map_err(|e| CtlError::InternalError(e.to_string()));
    }
    async fn history(self, _: context::Context, job_spec: Option<String>) -> Result<Vec<JobStatus>, CtlError> {
	self.get_history(job_spec)
	    .await
	    .map_err(|e| CtlError::InternalError(e.to_string()))
    }
    async fn status(self, _: context::Context, job: JobReference) -> Result<JobStatus, CtlError> {
	self.get_status(job)
	    .await
	    .map_err(|e| CtlError::InternalError(e.to_string()))
    }
    async fn log(self, _: context::Context, _job: JobReference) -> Result<Log, CtlError> {
	Err(CtlError::NotImplemented)
    }
    async fn cancel(self, _: context::Context, _job: JobReference) -> Result<JobStatus, CtlError> {
	Err(CtlError::NotImplemented)
    }

}

pub async fn spawn_socket(server_ctx: Arc<ServerCtx>) -> eyre::Result<()> {
    let mut incoming = unix::listen(&server_ctx.opts.ctl_socket_path, Json::default).await?;
    tokio::spawn(async move {
        while let Some(t) = incoming.next().await {
	    let transport = t.unwrap();
	    let cred = transport.get_ref().peer_cred().expect("Failed to retrieve peer credentials");
	    let service = CtlService { credentials: cred, server_ctx: Arc::clone(&server_ctx) };
            let fut = BaseChannel::with_defaults(transport)
                .execute(service.serve())
                .for_each(spawn);
            tokio::spawn(fut);
        }
    });

    Ok(())
}
