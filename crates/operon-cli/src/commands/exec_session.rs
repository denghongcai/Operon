use std::{io, path::PathBuf};

use anyhow::Context as _;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty as _,
};
use operon_core::{ExecSessionEvent, ExecSessionStart, ExecStatus};

use crate::{
    commands::exec_args,
    grpc_exec,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

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

pub(crate) async fn session(input: ExecSessionInput) -> anyhow::Result<()> {
    let endpoint = load_endpoint(input.config_path, &input.node_id)?;
    let dimensions = local_terminal_dimensions_or_default(input.rows, input.cols);
    let start = ExecSessionStart {
        command: if input.argv {
            String::new()
        } else {
            exec_args::command_from_cli_args(&input.command)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
