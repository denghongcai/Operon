use operon_core::ExecRunRequest;

pub(crate) fn command_from_cli_args(args: &[String]) -> String {
    if args.len() == 1 {
        return args[0].clone();
    }
    args.iter()
        .map(|arg| shell_escape_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn run_request_from_cli(
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
            command_from_cli_args(&command)
        },
        argv: if argv { command } else { Vec::new() },
        cwd,
        timeout_secs: Some(timeout_secs),
        secrets,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_command_preserves_single_shell_command_string() {
        let command = command_from_cli_args(&["echo hello | cat".to_string()]);

        assert_eq!(command, "echo hello | cat");
    }

    #[test]
    fn exec_command_shell_escapes_multiple_cli_args() {
        let command = command_from_cli_args(&[
            "printf".to_string(),
            "hello world".to_string(),
            "it's ok".to_string(),
        ]);

        assert_eq!(command, "printf 'hello world' 'it'\\''s ok'");
    }

    #[test]
    fn argv_exec_request_keeps_arguments_unescaped() {
        let request = run_request_from_cli(
            vec!["printf".to_string(), "hello world".to_string()],
            true,
            None,
            30,
            Vec::new(),
        );

        assert_eq!(request.command, "");
        assert_eq!(request.argv, vec!["printf", "hello world"]);
    }
}
