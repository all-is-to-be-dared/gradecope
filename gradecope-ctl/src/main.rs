use tarpc::{
    client, context,
    serde_transport::unix,
    tokio_serde::formats::Json,
};

use clap::{Parser, Subcommand};
use colored::Colorize;
use gradecope_proto::ctl::{CtlClient, JobReference, JobResult, JobStatus};
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

fn format_result(result: &JobResult) -> String {
    match result {
	JobResult::Pending => "⏳ Pending".yellow().to_string(),
	JobResult::Running => "⚙️  Running".cyan().to_string(),
	JobResult::Completed => "✓ Completed".green().to_string(),
	JobResult::Incorrect => "✗ Incorrect".red().to_string(),
	JobResult::Error => "⚠ Error".red().bold().to_string(),
	JobResult::Canceled => "⊘ Canceled".dimmed().to_string(),
	JobResult::Timeout => "⏱ Timeout".red().to_string(),
    }
}

fn print_job_status(status: &JobStatus) {
    println!("{}    {}", "Spec:".bold(), status.job_spec);
    println!("{}      {}", "ID:".bold(), status.job_id);
    println!("{}  {}", "Status:".bold(), format_result(&status.result));
}

fn print_job_table(jobs: &[JobStatus]) {
    if jobs.is_empty() {
	println!("{}", "No jobs found.".dimmed());
	return;
    }

    // Find max width for job_spec column
    let max_spec_width = jobs.iter().map(|j| j.job_spec.len()).max().unwrap_or(0);

    println!(
	"{:width$}  {:10}  {}",
	"JOB".bold().underline(),
	"ID".bold().underline(),
	"STATUS".bold().underline(),
	width = max_spec_width
    );

    for job in jobs {
	println!(
	    "{:width$}  {}  {}",
	    job.job_spec.bold(),
	    &job.job_id.to_string()[..8].dimmed(),
	    format_result(&job.result),
	    width = max_spec_width
	);
    }
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
		Ok(jobs) => print_job_table(&jobs),
		Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
	    }
	}
	Commands::Status { job_spec, id } => {
	    let job_ref = JobReference { job_spec, job_id: id };
	    match client.status(context::current(), job_ref).await? {
		Ok(status) => print_job_status(&status),
		Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
	    }
	}
	_ => eprintln!("{}", "Not implemented yet.".yellow()),
    }

    Ok(())
}
