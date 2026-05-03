use std::{
    io::{self, Write as _},
    path::PathBuf,
};

use anyhow::Context as _;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty as _,
};
use operon_core::{
    ExecList, ExecLogList, ExecRecord, ExecRunRequest, ExecSessionEvent, ExecSessionStart,
    ExecStatus, ExecStdin, ExecStdinClose,
};

use crate::{
    grpc, grpc_exec,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

pub(crate) struct ExecRunInput {
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

pub(crate) struct ExecSessionInput {
    pub(crate) config_path: PathBuf,
    pub(crate) node_id: String,
    pub(crate) cwd: Option<String>,
    pub(crate) timeout_secs: u64,
    pub(crate) secrets: Vec<String>,
    pub(crate) argv: bool,
    pub(crate) rows: Option<u16>,
    pub(crate) cols: Option<u16>,
    pub(crate) content: Option<String>,
    pub(crate) command: Vec<String>,
    pub(crate) output: OutputMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalDimensions {
    rows: u16,
    cols: u16,
}

pub(crate) async fn run(input: ExecRunInput) -> anyhow::Result<()> {
    let endpoint = load_endpoint(input.config_path.clone(), &input.node_id)?;
    let request = exec_run_request_from_cli(
        input.command,
        input.argv,
        input.cwd,
        input.timeout_secs,
        input.secrets,
    );
    let record: ExecRecord = grpc::run_exec(&endpoint, request).await?;
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
        let record = wait_for_exec(&endpoint, &record.id).await?;
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
    let list: ExecList = grpc::list_execs(&endpoint).await?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for record in list.execs {
        print_status(&record);
    }
    Ok(())
}

pub(crate) async fn status(
    config_path: PathBuf,
    node_id: &str,
    exec_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let record = load(config_path, node_id, exec_id).await?;
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
    exec_id: &str,
    follow: bool,
    stream: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    if output.json {
        let logs: ExecLogList = if stream || follow {
            grpc::stream_exec_logs(&endpoint, exec_id).await?
        } else {
            grpc::list_exec_logs(&endpoint, exec_id).await?
        };
        print_json(&logs)?;
        return Ok(());
    }
    if stream || follow {
        if output.quiet {
            return grpc_exec::stream_exec_logs_to_writer(&endpoint, exec_id, &mut io::sink())
                .await;
        }
        return grpc_exec::stream_exec_logs_to_writer(&endpoint, exec_id, &mut io::stdout()).await;
    }
    let logs = grpc::list_exec_logs(&endpoint, exec_id).await?;
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
    exec_id: &str,
    content: Option<String>,
    file: Option<PathBuf>,
    close: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    if close {
        let closed: ExecStdinClose = grpc::close_exec_stdin(&endpoint, exec_id).await?;
        if output.json {
            print_json(&closed)?;
        } else if !output.quiet {
            println!("{} stdin_closed={}", closed.exec_id, closed.closed);
        }
        return Ok(());
    }
    let written: ExecStdin = match (content, file) {
        (Some(content), None) => {
            grpc::write_exec_stdin_bytes(&endpoint, exec_id, content.as_bytes()).await?
        }
        (None, Some(file)) => grpc::write_exec_stdin_file(&endpoint, exec_id, &file).await?,
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("exec stdin requires --content, --file, or --close"),
    };
    if output.json {
        print_json(&written)?;
    } else if !output.quiet {
        println!(
            "{} stdin_bytes_written={}",
            written.exec_id, written.bytes_written
        );
    }
    Ok(())
}

pub(crate) async fn session(input: ExecSessionInput) -> anyhow::Result<()> {
    let endpoint = load_endpoint(input.config_path, &input.node_id)?;
    let dimensions = local_terminal_dimensions_or_default(input.rows, input.cols);
    let start = ExecSessionStart {
        command: if input.argv {
            String::new()
        } else {
            exec_command_from_cli_args(&input.command)
        },
        argv: if input.argv {
            input.command
        } else {
            Vec::new()
        },
        cwd: input.cwd,
        timeout_secs: Some(input.timeout_secs),
        secrets: input.secrets,
        rows: dimensions.rows,
        cols: dimensions.cols,
    };
    let mut stdout = io::stdout();
    let event = match input.content {
        Some(content) => {
            grpc_exec::open_exec_session_to_writer(
                &endpoint,
                start,
                grpc_exec::ExecSessionInputSource::Inline(content.into_bytes()),
                &mut stdout,
            )
            .await?
        }
        None => {
            let _raw_mode = RawModeGuard::enable_if_tty()?;
            grpc_exec::open_exec_session_to_writer(
                &endpoint,
                start,
                grpc_exec::ExecSessionInputSource::LocalStdin {
                    forward_resize: io::stdin().is_tty(),
                },
                &mut stdout,
            )
            .await?
        }
    };
    finish_session(event, input.output)
}

fn finish_session(event: ExecSessionEvent, output: OutputMode) -> anyhow::Result<()> {
    if output.json {
        print_json(&event)?;
    } else if !output.quiet {
        print_session_terminal(&event);
    }
    ensure_session_succeeded(&event)
}

pub(crate) fn local_terminal_dimensions_or_default(
    rows: Option<u16>,
    cols: Option<u16>,
) -> TerminalDimensions {
    let terminal_size = io::stdin()
        .is_tty()
        .then(crossterm::terminal::size)
        .and_then(Result::ok);
    TerminalDimensions {
        rows: rows
            .or_else(|| terminal_size.map(|(_, rows)| rows))
            .unwrap_or(24),
        cols: cols
            .or_else(|| terminal_size.map(|(cols, _)| cols))
            .unwrap_or(80),
    }
}

fn ensure_session_succeeded(event: &ExecSessionEvent) -> anyhow::Result<()> {
    match event {
        ExecSessionEvent::Exit(exit) if matches!(exit.status, ExecStatus::Succeeded) => Ok(()),
        ExecSessionEvent::Exit(exit) => {
            let exit_code = exit
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string());
            anyhow::bail!(
                "exec session {} ended with status {:?} exit_code={}",
                exit.exec_id,
                exit.status,
                exit_code
            )
        }
        _ => anyhow::bail!("exec session ended without exit event"),
    }
}

fn print_session_terminal(event: &ExecSessionEvent) {
    if let ExecSessionEvent::Exit(exit) = event {
        eprintln!(
            "{} session {:?} exit_code={:?}",
            exit.exec_id, exit.status, exit.exit_code
        );
    }
}

struct RawModeGuard {
    enabled: bool,
}

impl RawModeGuard {
    fn enable_if_tty() -> anyhow::Result<Self> {
        let enabled = io::stdin().is_tty();
        if enabled {
            enable_raw_mode().context("enable local terminal raw mode")?;
        }
        Ok(Self { enabled })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.enabled {
            let _ = disable_raw_mode();
        }
    }
}

pub(crate) async fn cancel(
    config_path: PathBuf,
    node_id: &str,
    exec_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let record: ExecRecord = grpc::cancel_exec(&endpoint, exec_id).await?;
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
    exec_id: &str,
) -> anyhow::Result<ExecRecord> {
    let endpoint = load_endpoint(config_path, node_id)?;
    grpc::get_exec(&endpoint, exec_id).await
}

fn exec_command_from_cli_args(args: &[String]) -> String {
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

fn exec_run_request_from_cli(
    command: Vec<String>,
    argv: bool,
    cwd: Option<String>,
    timeout_secs: u64,
    secrets: Vec<String>,
) -> ExecRunRequest {
    ExecRunRequest {
        command: if argv {
            String::new()
        } else {
            exec_command_from_cli_args(&command)
        },
        argv: if argv { command } else { Vec::new() },
        cwd,
        timeout_secs: Some(timeout_secs),
        secrets,
    }
}

async fn wait_for_exec(
    endpoint: &operon_network::NodeEndpoint,
    exec_id: &str,
) -> anyhow::Result<ExecRecord> {
    let _ = grpc::watch_exec_to_terminal(endpoint, exec_id).await?;
    grpc::get_exec(endpoint, exec_id).await
}

async fn print_logs(endpoint: &operon_network::NodeEndpoint, exec_id: &str) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    for log in grpc::list_exec_logs(endpoint, exec_id).await?.logs {
        stdout.write_all(&log.data)?;
    }
    Ok(())
}

fn ensure_succeeded(record: &ExecRecord) -> anyhow::Result<()> {
    match record.status {
        ExecStatus::Succeeded => Ok(()),
        ExecStatus::Running => anyhow::bail!("exec {} is still running", record.id),
        ExecStatus::Failed | ExecStatus::Cancelled | ExecStatus::TimedOut => {
            let exit_code = record
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string());
            anyhow::bail!(
                "exec {} ended with status {:?} exit_code={}",
                record.id,
                record.status,
                exit_code
            )
        }
    }
}

fn print_status(record: &ExecRecord) {
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
    fn exec_command_preserves_single_shell_command_string() {
        let command = exec_command_from_cli_args(&["echo hello | cat".to_string()]);

        assert_eq!(command, "echo hello | cat");
    }

    #[test]
    fn exec_command_shell_escapes_multiple_cli_args() {
        let command = exec_command_from_cli_args(&[
            "printf".to_string(),
            "hello world".to_string(),
            "it's ok".to_string(),
        ]);

        assert_eq!(command, "printf 'hello world' 'it'\\''s ok'");
    }

    #[test]
    fn argv_exec_request_keeps_arguments_unescaped() {
        let request = exec_run_request_from_cli(
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
    fn exec_session_terminal_dimensions_use_explicit_overrides() {
        let dimensions = local_terminal_dimensions_or_default(Some(33), Some(120));

        assert_eq!(
            dimensions,
            TerminalDimensions {
                rows: 33,
                cols: 120
            }
        );
    }

    #[test]
    fn exec_session_terminal_dimensions_default_when_unattached() {
        let dimensions = local_terminal_dimensions_or_default(None, None);

        assert!(dimensions.rows > 0);
        assert!(dimensions.cols > 0);
    }

    #[test]
    fn failed_terminal_exec_returns_cli_error() {
        let record = test_exec_record(ExecStatus::Failed, Some(1));

        let error = ensure_succeeded(&record).expect_err("failed exec should error");

        assert!(error.to_string().contains("ended with status Failed"));
    }

    #[test]
    fn succeeded_terminal_exec_is_ok() {
        let record = test_exec_record(ExecStatus::Succeeded, Some(0));

        ensure_succeeded(&record).expect("succeeded exec should be ok");
    }

    fn test_exec_record(status: ExecStatus, exit_code: Option<i32>) -> ExecRecord {
        ExecRecord {
            id: "exec-1".to_string(),
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
