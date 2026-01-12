use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::Utc;
use gradecope_proto::runner::{JobResult, JobSpec, JobTermination, Log};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

#[tracing::instrument(
    fields(
        job.id = %spec.id, job.repo = spec.repo_path, job.commit = spec.commit_hash,
        job.spec = spec.job_spec, worker.id = %worker_id),
    skip(test_runner, cancel, output)
)]
pub async fn run_job(
    worker_id: Uuid,
    dev_ctl: crate::DeviceCtl,
    test_runner: PathBuf,
    spec: JobSpec,
    mut cancel: tokio::sync::oneshot::Receiver<()>,
    output: tokio::sync::oneshot::Sender<JobTermination>,
) {
    let setup_args = |cmd: &mut tokio::process::Command, logfile: &Path| {
        cmd.arg(worker_id.to_string());
        cmd.arg(spec.id.to_string());
        cmd.arg(&spec.repo_path);
        cmd.arg(&spec.commit_hash);
        cmd.arg(logfile);
        cmd.arg(&dev_ctl.serial);
        let port_numbers = dev_ctl.usb_dev.port_numbers().unwrap();
        let (last_port, port_prefix) = port_numbers.split_last().unwrap();
        let last_port_str = format!("{last_port}");
        let mut port_prefix_str = format!("{}-", dev_ctl.usb_dev.bus_number());
        for s in port_prefix.iter().map(Some).intersperse(None) {
            if let Some(s) = s {
                port_prefix_str.push_str(&format!("{s}"));
            } else {
                port_prefix_str.push('.');
            }
        }
        cmd.arg(last_port_str);
        cmd.arg(port_prefix_str);
    };

    let result = 'run: {
        let logfile = match async_tempfile::TempFile::new().await {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Failed to create temporary log file for job: {e:?}");
                break 'run JobTermination {
                    job_id: spec.id,
                    log: Log {
                        log: vec![],
                        truncated: false,
                    },
                    result: JobResult::Error,
                    now: Utc::now(),
                };
            }
        };

        // runner command

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg(test_runner.join(format!("{}-run.sh", spec.job_spec)));
        setup_args(&mut cmd, logfile.file_path());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    "Failed to spawn {}-cleanup.sh process: {e:?}",
                    spec.job_spec
                );
                break 'run JobTermination {
                    job_id: spec.id,
                    log: Log {
                        log: vec![],
                        truncated: false,
                    },
                    result: JobResult::Error,
                    now: Utc::now(),
                };
            }
        };
        let pid = child.id();

        let timeout = tokio::time::sleep(Duration::from_secs(120));

        // if let Err(e) = output.try_send(WorkerMsg::Started {
        //     job_id: spec.id,
        //     now: Utc::now(),
        // }) {
        //     tracing::error!("Worker failed to send job-started message: {e:?}, killing worker");
        //     return;
        // }

        tokio::pin!(timeout);
        let result = tokio::select! {
            biased;
            _ = &mut timeout => {
                // timed out
                if let Err(e) = child.kill().await {
                    tracing::error!("Failed to SIGKILL {}-run.sh process with PID {pid:?}: {e:?}", spec.job_spec);
                }
                JobResult::Timeout
            }
            _ = &mut cancel => {
                if let Err(e) = child.kill().await {
                    tracing::error!("Failed to SIGKILL {}-run.sh process with PID {pid:?}: {e:?}", spec.job_spec);
                }
                JobResult::Canceled
            }
            res = child.wait() => {
                match res {
                    Ok(exit_status) => {
                        exit_status.code().map(|code| match code {
                            0 => JobResult::Correct,
                            1 => JobResult::Incorrect,
                            2 => JobResult::Error,
                            3 => JobResult::Canceled,
                            4 => JobResult::Timeout,
                            other => {
                                tracing::error!("Unrecognized exit code from {}-run.sh: {other}", spec.job_spec);
                                JobResult::Error
                            }
                        }).unwrap_or_else(|| {
                            tracing::error!("");
                            JobResult::Error})
                    },
                    Err(e) => {
                        tracing::error!("Error wait()-ing for {}-run.sh: {e:?}", spec.job_spec);
                        JobResult::Error
                    },
                }
            }
        };

        // cleanup command

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg(test_runner.join(format!("{}-cleanup.sh", spec.job_spec)));
        setup_args(&mut cmd, logfile.file_path());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    "Failed to spawn {}-cleanup.sh process: {e:?}, killing worker",
                    spec.job_spec
                );
                break 'run JobTermination {
                    job_id: spec.id,
                    log: Log {
                        log: vec![],
                        truncated: false,
                    },
                    result: JobResult::Error,
                    now: Utc::now(),
                };
            }
        };
        let pid = child.id();
        let timeout = tokio::time::sleep(Duration::from_secs(45));
        tokio::pin!(timeout);
        tokio::select! {
            biased;
            _ = &mut timeout => {
                if let Err(e) = child.kill().await {
                    tracing::error!("Failed to SIGKILL {}-cleanup.sh process with PID {pid:?}: {e:?}, killing worker", spec.job_spec);
                }
            }
            res = child.wait() => {
                match res {
                    Ok(exit_status) => {
                        if !exit_status.success() {
                            tracing::error!("{}-cleanup.sh process exited unsuccesfully with exit code {exit_status}, killing worker", spec.job_spec);
                            break 'run JobTermination {
                                job_id: spec.id,
                                log: Log {
                                    log: vec![],
                                    truncated: false,
                                },
                                result: JobResult::Error,
                                now: Utc::now(),
                            };
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to wait() for {}-cleanup.sh process with PID {pid:?}: {e:?}, killing worker", spec.job_spec);
                        break 'run JobTermination {
                            job_id: spec.id,
                            log: Log {
                                log: vec![],
                                truncated: false,
                            },
                            result: JobResult::Error,
                            now: Utc::now(),
                        };
                    }
                }
            }
        }

        // read log file

        // 64K is enough for anybody
        const LOG_LIMIT: usize = 1024 * 64;
        let mut v = vec![];
        let log = match logfile.take(LOG_LIMIT as u64 + 1).read_to_end(&mut v).await {
            Ok(s) if s > LOG_LIMIT => {
                v.truncate(LOG_LIMIT);
                Log {
                    log: v,
                    truncated: true,
                }
            }
            Ok(_) => Log {
                log: v,
                truncated: false,
            },
            Err(e) => {
                tracing::error!("Failed to read log file: {e:?}");
                Log {
                    log: vec![],
                    truncated: true,
                }
            }
        };

        JobTermination {
            job_id: spec.id,
            log,
            result,
            now: Utc::now(),
        }
    };
    if let Err(_) = output.send(result) {
        tracing::error!("Failed to send job termination: dispatcher channel closed");
    }
}
