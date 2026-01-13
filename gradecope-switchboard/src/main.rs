#![feature(try_blocks)]

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

use clap::Parser;
use sqlx::{PgPool, postgres::PgConnectOptions};

mod runner;
mod sql;
mod submission;

#[derive(Debug, Parser)]
pub struct Opts {
    // --- QUOTAS ---
    /// Maximum number of submitted jobs per hour per user
    #[arg(long, default_value_t = 120)]
    quota_jobs_per_hr: u32,
    /// Maximum number of concurrent jobs per user
    #[arg(long, default_value_t = 4)]
    quota_max_concurrent_jobs: u32,

    // --- PATH CONTROLS ---
    /// Path to the directory where user account home directories are located.
    #[arg(long, default_value = "/home")]
    home_prefix: PathBuf,
    /// Path, relative to user account home directories, where repo is located
    #[arg(long, default_value = "gradecope-repo")]
    repo_path: String,
    /// Path, relative to user account home directories, where submit socket is located
    #[arg(long, default_value = "gradecope-sockets/submit.sock")]
    submit_socket_path: String,
    /// Path of admin socket
    #[arg(long, default_value = "/home/gradecope/gradecope-admin.sock")]
    admin_socket_path: String,

    // --- RUNNER SERVER CONFIG ---
    /// Address to which the thin WebSocket server for the runner to call home to should be bound.
    #[arg(long, default_value_t = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10121))]
    bind_server: SocketAddr,
}

pub struct ServerCtx {
    opts: Opts,
    pool: PgPool,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    // --- Check that home_prefix is a directory
    if !opts.home_prefix.exists() {
        tracing::error!(
            "Expected user home directory prefix to exist, but {home_prefix} does not",
            home_prefix = opts.home_prefix.display()
        );
        return;
    }
    if !opts.home_prefix.is_dir() {
        tracing::error!(
            "Expected user home directory prefix to be a directory but {home_prefix} is not",
            home_prefix = opts.home_prefix.display()
        );
        return;
    }

    // --- Open database connection pool
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .connect_with(PgConnectOptions::new())
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to connect to database: {e:?}");
            return;
        }
    };

    // --- Create server context
    let server_ctx = Arc::new(ServerCtx { opts, pool });

    // there are a few different components we have to handle:
    //  1. submission socket listening
    //  2. runner socket listening (websocket)
    //  3. student socket listening
    //  4. admin socket listening
    //
    // submission sockets push directly to the database job queue if within quotas
    // runner socket is more complicated
    // student socket is quite simple, just wait for command then dump JSON
    // admin socket is also simple, if someone writes to the admin socket we just add a submission
    //      socket listener

    // --- Start up submission socket listeners for all users currently in the database
    let submit_listeners = match submission::spawn_socket_listeners(server_ctx.clone()).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to spawn submission socket listeners: {e:?}");
            return;
        }
    };

    let runner_handler = match runner::spawn_handler(server_ctx.clone()).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to serve runner control server: {e:?}");
            submit_listeners.close().await;
            return;
        }
    };

    loop {}

    // // --- Shut down submission socket listeners
    // submit_listeners.close().await;
}
