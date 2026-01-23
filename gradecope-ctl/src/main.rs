use std::io::{self, Write};
use std::process::{Command, Stdio};

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
	id: Uuid,
	/// Print to stdout instead of using a pager
	#[arg(long)]
	no_pager: bool,
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

fn last_n_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn print_job_status(status: &JobStatus, log_preview: Option<&str>) {
    println!("{}    {}", "Spec:".bold(), status.job_spec);
    println!("{}      {}", "ID:".bold(), status.job_id);
    println!("{}  {}", "Status:".bold(), format_result(&status.result));

    if let Some(log) = log_preview {
	println!();
	println!("{}", "Last 10 lines of log:".bold().underline());
	println!("{}", log.dimmed());
    }
}

fn print_job_table(jobs: &[JobStatus]) {
    if jobs.is_empty() {
	println!("{}", "No jobs found.".dimmed());
	return;
    }

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

fn show_in_pager(content: &[u8]) -> io::Result<()> {
    for pager in ["less", "more"] {
	if let Ok(mut child) = Command::new(pager).stdin(Stdio::piped()).spawn() {
	    if let Some(mut stdin) = child.stdin.take() {
		stdin.write_all(content)?;
	    }
	    child.wait()?;
	    return Ok(());
	}
    }
    io::stdout().write_all(content)
}

fn print_error(e: impl std::fmt::Display) {
    eprintln!("{} {e}", "Error:".red().bold());
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts = Opts::parse();

    let transport = unix::connect(opts.ctl_socket_path, Json::default).await?;
    let client = CtlClient::new(client::Config::default(), transport).spawn();

    match opts.command {
	Commands::Hi => println!("{}", client.hi(context::current()).await?),

	Commands::History { job_spec } => {
	    match client.history(context::current(), job_spec).await? {
		Ok(jobs) => print_job_table(&jobs),
		Err(e) => print_error(e),
	    }
	}

	Commands::Status { job_spec, id } => {
	    let job_ref = JobReference { job_spec, job_id: id };
	    match client.status(context::current(), job_ref.clone()).await? {
		Ok(status) => {
		    let log_preview = client.log(context::current(), job_ref).await
			.ok()
			.and_then(|r| r.ok())
			.map(|log| String::from_utf8_lossy(&log.log).into_owned())
			.filter(|s| !s.is_empty())
			.map(|s| last_n_lines(&s, 10));
		    print_job_status(&status, log_preview.as_deref());
		}
		Err(e) => print_error(e),
	    }
	}

	Commands::Log { job_spec, id, no_pager } => {
	    let job_ref = JobReference { job_spec, job_id: id };
	    match client.log(context::current(), job_ref).await? {
		Ok(log) if log.log.is_empty() => println!("{}", "No log available.".dimmed()),
		Ok(log) if no_pager => { io::stdout().write_all(&log.log)?; }
		Ok(log) => { show_in_pager(&log.log)?; }
		Err(e) => print_error(e),
	    }
	}

	_ => eprintln!("{}", "Not implemented yet.".yellow()),
    }

    Ok(())
}
