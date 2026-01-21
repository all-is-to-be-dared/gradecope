use tarpc::{
    client, context,
    serde_transport::unix,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Json,
};

use clap::{Parser, Subcommand};
use gradecope_proto::ctl::{Ctl, CtlClient};

#[derive(Debug, Parser)]
#[command(name = "gradecope-ctl")]
struct Opts {
    #[arg(long, default_value = "/home/gradecope/gradecope-ctl.sock")]
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
	id: u32
    },
    Log {
	job_spec: String,
	id: u32
    },
    Cancel {
	job_spec: String,
	id: u32
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
	_ => println!("Unimplemented!"),
    }

    Ok(())
}
