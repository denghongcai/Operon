use std::{
    io::{self, Write as _},
    path::PathBuf,
};

use operon_core::{
    JobList, JobLogList, JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose,
};

use crate::{
    grpc,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

pub(crate) struct JobRunInput {
    pub(crate) config_path: PathBuf,
    pub(crate) node_id: String,
    pub(crate) cwd: Option<String>,
    pub(crate) timeout_secs: u64,
    pub(crate) secrets: Vec<String>,
    pub(crate) detach: bool,
    pub(crate) argv: bool,
    pub(crate) command: Vec<String>,
    pub(crate) output: OutputMode,
}

pub(crate) async fn run(input: JobRunInput) -> anyhow::Result<()> {
    let endpoint = load_endpoint(input.config_path.clone(), &input.node_id)?;
    let request = job_run_request_from_cli(
        input.command,
        input.argv,
        input.cwd,
        input.timeout_secs,
        input.secrets,
    );
    let record: JobRecord = grpc::run_job(&endpoint, request).await?;
    if input.detach {
        if input.output.json {
            print_json(&record)?;
        } else if !input.output.quiet {
            println!(
                "{} {} {:?} {}",
                record.node_id, record.id, record.status, record.command
            );
        }
    }

    if !input.detach {
        let record = wait_for_job(&endpoint, &record.id).await?;
        if input.output.json {
            print_json(&record)?;
        } else if !input.output.quiet {
            print_status(&record);
            print_logs(&endpoint, &record.id).await?;
        }
        ensure_succeeded(&record)?;
    }

    Ok(())
}

pub(crate) async fn list(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let list: JobList = grpc::list_jobs(&endpoint).await?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for record in list.jobs {
        print_status(&record);
    }
    Ok(())
}

pub(crate) async fn status(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let record = load(config_path, node_id, job_id).await?;
    if output.json {
        print_json(&record)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_status(&record);
    Ok(())
}

pub(crate) async fn logs(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    follow: bool,
    stream: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    if output.json {
        let logs: JobLogList = if stream || follow {
            grpc::stream_job_logs(&endpoint, job_id).await?
        } else {
            grpc::list_job_logs(&endpoint, job_id).await?
        };
        print_json(&logs)?;
        return Ok(());
    }
    if stream || follow {
        if output.quiet {
            return grpc::stream_job_logs_to_writer(&endpoint, job_id, &mut io::sink()).await;
        }
        return grpc::stream_job_logs_to_writer(&endpoint, job_id, &mut io::stdout()).await;
    }
    let logs = grpc::list_job_logs(&endpoint, job_id).await?;
    if output.quiet {
        return Ok(());
    }
    let mut stdout = io::stdout();
    for log in logs.logs {
        stdout.write_all(&log.data)?;
    }
    Ok(())
}

pub(crate) async fn stdin(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    content: Option<String>,
    file: Option<PathBuf>,
    close: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    if close {
        let closed: JobStdinClose = grpc::close_job_stdin(&endpoint, job_id).await?;
        if output.json {
            print_json(&closed)?;
        } else if !output.quiet {
            println!("{} stdin_closed={}", closed.job_id, closed.closed);
        }
        return Ok(());
    }
    let written: JobStdin = match (content, file) {
        (Some(content), None) => {
            grpc::write_job_stdin_bytes(&endpoint, job_id, content.as_bytes()).await?
        }
        (None, Some(file)) => grpc::write_job_stdin_file(&endpoint, job_id, &file).await?,
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("job stdin requires --content, --file, or --close"),
    };
    if output.json {
        print_json(&written)?;
    } else if !output.quiet {
        println!(
            "{} stdin_bytes_written={}",
            written.job_id, written.bytes_written
        );
    }
    Ok(())
}

pub(crate) async fn cancel(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let record: JobRecord = grpc::cancel_job(&endpoint, job_id).await?;
    if output.json {
        print_json(&record)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_status(&record);
    Ok(())
}

pub(crate) async fn load(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
) -> anyhow::Result<JobRecord> {
    let endpoint = load_endpoint(config_path, node_id)?;
    grpc::get_job(&endpoint, job_id).await
}

fn job_command_from_cli_args(args: &[String]) -> String {
    if args.len() == 1 {
        return args[0].clone();
    }
    args.iter()
        .map(|arg| shell_escape_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'=' | b'@')
    }) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn job_run_request_from_cli(
    command: Vec<String>,
    argv: bool,
    cwd: Option<String>,
    timeout_secs: u64,
    secrets: Vec<String>,
) -> JobRunRequest {
    JobRunRequest {
        command: if argv {
            String::new()
        } else {
            job_command_from_cli_args(&command)
        },
        argv: if argv { command } else { Vec::new() },
        cwd,
        timeout_secs: Some(timeout_secs),
        secrets,
    }
}

async fn wait_for_job(
    endpoint: &operon_network::NodeEndpoint,
    job_id: &str,
) -> anyhow::Result<JobRecord> {
    let _ = grpc::watch_job_to_terminal(endpoint, job_id).await?;
    grpc::get_job(endpoint, job_id).await
}

async fn print_logs(endpoint: &operon_network::NodeEndpoint, job_id: &str) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    for log in grpc::list_job_logs(endpoint, job_id).await?.logs {
        stdout.write_all(&log.data)?;
    }
    Ok(())
}

fn ensure_succeeded(record: &JobRecord) -> anyhow::Result<()> {
    match record.status {
        JobStatus::Succeeded => Ok(()),
        JobStatus::Running => anyhow::bail!("job {} is still running", record.id),
        JobStatus::Failed | JobStatus::Cancelled | JobStatus::TimedOut => {
            let exit_code = record
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string());
            anyhow::bail!(
                "job {} ended with status {:?} exit_code={}",
                record.id,
                record.status,
                exit_code
            )
        }
    }
}

fn print_status(record: &JobRecord) {
    println!(
        "{} {} {:?} exit_code={:?} logs={} truncated={}",
        record.node_id,
        record.id,
        record.status,
        record.exit_code,
        record.log_count,
        record.logs_truncated
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_command_preserves_single_shell_command_string() {
        let command = job_command_from_cli_args(&["echo hello | cat".to_string()]);

        assert_eq!(command, "echo hello | cat");
    }

    #[test]
    fn job_command_shell_escapes_multiple_cli_args() {
        let command = job_command_from_cli_args(&[
            "printf".to_string(),
            "hello world".to_string(),
            "it's ok".to_string(),
        ]);

        assert_eq!(command, "printf 'hello world' 'it'\\''s ok'");
    }

    #[test]
    fn argv_job_request_keeps_arguments_unescaped() {
        let request = job_run_request_from_cli(
            vec!["printf".to_string(), "hello world".to_string()],
            true,
            None,
            30,
            Vec::new(),
        );

        assert_eq!(request.command, "");
        assert_eq!(request.argv, vec!["printf", "hello world"]);
    }

    #[test]
    fn failed_terminal_job_returns_cli_error() {
        let record = test_job_record(JobStatus::Failed, Some(1));

        let error = ensure_succeeded(&record).expect_err("failed job should error");

        assert!(error.to_string().contains("ended with status Failed"));
    }

    #[test]
    fn succeeded_terminal_job_is_ok() {
        let record = test_job_record(JobStatus::Succeeded, Some(0));

        ensure_succeeded(&record).expect("succeeded job should be ok");
    }

    fn test_job_record(status: JobStatus, exit_code: Option<i32>) -> JobRecord {
        JobRecord {
            id: "job-1".to_string(),
            node_id: "local".to_string(),
            command: "false".to_string(),
            cwd: "/".to_string(),
            status,
            exit_code,
            log_count: 0,
            logs_truncated: false,
        }
    }
}
