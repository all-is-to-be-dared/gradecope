use tarpc::{
    client, context,
    serde_transport::unix,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Json,
};

use clap::{Parser, Subcommand};
use gradecope_proto::ctl::{Ctl, CtlClient, JobReference};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "gradecope-ctl")]
struct Opts {
    #[arg(long, default_value = "/var/run/gradecope/gradecope-ctl.sock")]
    ctl_socket_path: String,
    #[command(subcommand)]
    command: Commands
}

#[derive(Debug, Subcommand)]
enum Commands {
    Hi,
    Submit {
	job_spec: String,
	commit: String,
    },
    History {
	job_spec: Option<String>
    },
    Status {
	job_spec: String,
	id: Uuid
    },
    Log {
	job_spec: String,
	id: Uuid
    },
    Cancel {
	job_spec: String,
	id: Uuid
    },
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts = Opts::parse();

    // Connect to socket and make request
    let transport = unix::connect(opts.ctl_socket_path, Json::default).await?;
    let client = CtlClient::new(client::Config::default(), transport).spawn();

    match opts.command {
	Commands::Hi => println!("{}", client.hi(context::current()).await?),
	Commands::History { job_spec } => {
	    match client.history(context::current(), job_spec).await? {
		Ok(jobs) => {
		    if jobs.is_empty() {
			println!("No jobs found.");
		    } else {
			for job in jobs {
			    println!("{}\t{}\t{:?}", job.job_spec, job.job_id, job.result);
			}
		    }
		}
		Err(e) => eprintln!("Error: {e}"),
	    }
	}
	Commands::Status { job_spec, id } => {
	    let job_ref = JobReference { job_spec, job_id: id };
	    match client.status(context::current(), job_ref).await? {
		Ok(status) => {
		    println!("{}\t{}\t{:?}", status.job_spec, status.job_id, status.result);
		}
		Err(e) => eprintln!("Error: {e}"),
	    }
	}
	_ => println!("Unimplemented!"),
    }

    Ok(())
}
