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
    History {
	jobspec: Option<String>
    },
    Status {
	jobspec: String,
	id: u32
    },
    Log {
	jobspec: String,
	id: u32
    },
    Cancel {
	jobspec: String,
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
	_ => println!("Unimplemented!"),
    }

    Ok(())
}
