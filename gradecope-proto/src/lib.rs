pub mod runner {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub enum JobResponse {
        Job {
            /// Job ID
            id: uuid::Uuid,
            /// Path to repository on remote
            repo_path: String,
            /// Commit hash to build on
            commit_hash: String,
            /// Job spec
            job_spec: String,
        },
        Unavailable,
    }

    #[tarpc::service]
    pub trait Switchboard {
        /// Request a job from the switchboard.
        async fn request_job() -> JobResponse;
    }
}
