use std::process::Stdio;

use tokio::process::Command as TokioCommand;

use crate::state::ExecTask;

pub(crate) fn build_exec_command(task: &ExecTask) -> TokioCommand {
    let mut command = if task.argv.is_empty() {
        let mut command = TokioCommand::new(exec_shell_program());
        command.arg(exec_shell_arg()).arg(&task.command);
        command
    } else {
        let mut command = TokioCommand::new(&task.argv[0]);
        command.args(&task.argv[1..]);
        command
    };
    command
        .current_dir(&task.cwd)
        .env_clear()
        .envs(&task.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_exec_process_group(&mut command);
    command
}

#[cfg(windows)]
fn exec_shell_program() -> &'static str {
    "cmd.exe"
}

#[cfg(not(windows))]
fn exec_shell_program() -> &'static str {
    "/bin/sh"
}

#[cfg(windows)]
fn exec_shell_arg() -> &'static str {
    "/C"
}

#[cfg(not(windows))]
fn exec_shell_arg() -> &'static str {
    "-c"
}

#[cfg(unix)]
fn configure_exec_process_group(command: &mut TokioCommand) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_exec_process_group(_command: &mut TokioCommand) {}

#[cfg(test)]
mod tests {
    #[test]
    fn exec_shell_invocation_matches_platform() {
        #[cfg(windows)]
        {
            assert_eq!(super::exec_shell_program(), "cmd.exe");
            assert_eq!(super::exec_shell_arg(), "/C");
        }
        #[cfg(not(windows))]
        {
            assert_eq!(super::exec_shell_program(), "/bin/sh");
            assert_eq!(super::exec_shell_arg(), "-c");
        }
    }
}
