pub mod runner {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct JobSpec {
        /// Job ID
        pub id: uuid::Uuid,
        /// Path to repository on remote
        pub repo_path: String,
        /// Commit hash to build on
        pub commit_hash: String,
        /// Job spec
        pub job_spec: String,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub enum JobResponse {
        Job(JobSpec),
        Unavailable,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub enum JobResult {
        Correct,
        Incorrect,
        Error,
        Canceled,
        Timeout,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Log {
        pub log: Vec<u8>,
        pub truncated: bool,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct JobTermination {
        pub job_id: uuid::Uuid,
        pub log: Log,
        pub result: JobResult,
        pub now: DateTime<Utc>,
    }

    #[tarpc::service]
    pub trait Switchboard {
        /// Request a job from the switchboard.
        async fn request_job() -> JobResponse;

        /// Notify the switchboard that the given job has stopped running, whether that's due to
        /// running to completion or to be canceled / having an error.
        async fn job_stopped(termination: JobTermination);

        /// Request that the switchboard tell the client the IDs of any jobs currently assigned to
        /// the client that were canceled, but have not yet stopped.
        async fn request_cancellation_notifications(
            currently_running: Vec<uuid::Uuid>,
        ) -> Vec<uuid::Uuid>;
    }
}

pub mod ctl {
    use tarpc::context;
    use serde::{Deserialize, Serialize};
    use thiserror::Error;

    #[derive(Debug, Clone, Deserialize, Serialize, Error)]
    pub enum CtlError {
	/// Permission denied. Should only be returned when trying to use admin
	/// commands, not when accessing a resource (prevents information
	/// leakage)
	#[error("permission denied")]
	PermissionDenied,
	#[error("not found: {0}")]
	NotFound(String),
	#[error("internal error: {0}")]
	InternalError(String)
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Submission {
        pub commit: String,
        pub spec: String,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct JobReference {
	pub job_spec: String,
	pub job_id: uuid::Uuid;
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Log {
        pub log: Vec<u8>,
        pub truncated: bool,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub enum JobResult {
        Correct,
        Incorrect,
        Error,
        Canceled,
        Timeout,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct JobStatus {
	pub job_spec: String,
	pub job_id: uuid::Uuid;
	pub result: JobResult;
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Log {
        pub log: Vec<u8>,
        pub truncated: bool,
    }

    #[tarpc::service]
    pub trait Ctl {
	async fn hi() -> String;
	async fn submit(commit: String, job_spec: String) -> Result<(), CtlError>;
	/// Return job history
	///
	/// If given a job spec, returns all jobs for that job spec. Otherwise,
	/// returns most recent job for each job spec
	async fn history(job_spec: Option<String>) -> Result<Vec<JobStatus>, CtlError>;
	async fn status(job: JobReference) -> Result<JobStatus, CtlError>;
	async fn log(job: JobReference) -> Result<Log, CtlError>;
    } 
}
