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

pub mod submit {
    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    pub struct Submission {
        pub user: String,
        pub commit: String,
        pub spec: String,
    }
}