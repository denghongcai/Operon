use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{atomic::Ordering, mpsc as std_mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use operon_core::{
    ExecLog, ExecRecord, ExecSessionEvent, ExecSessionExit, ExecSessionOutput, ExecSessionStart,
    ExecSessionStarted, ExecStatus,
};
use operon_fs::resolve_existing_workspace_path;
use operon_process::{
    authorize_exec_session_decision, exec_environment, resolve_exec_secrets_decision,
};
use operon_protocol::runtime::v1::{
    exec_session_event, exec_session_request, ExecSessionEvent as GrpcExecSessionEvent,
    ExecSessionRequest,
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::{sync::mpsc, task};
use tonic::{Status, Streaming};

use crate::{
    audit::{current_request_context, record_audit_capability, record_policy_decision},
    exec_runtime::{append_exec_log, finish_exec},
    grpc_status::status_from_error,
    locks::lock,
    state::{AppState, ExecCompletion, ExecLogBuffer, ExecLogSender},
};

pub(crate) type ExecSessionStream = std::pin::Pin<
    Box<dyn futures_util::Stream<Item = Result<GrpcExecSessionEvent, Status>> + Send + 'static>,
>;

enum SessionControl {
    Input(Vec<u8>),
    Resize(PtySize),
    CloseInput,
    Terminate,
}

pub(crate) async fn open_exec_session(
    state: AppState,
    mut stream: Streaming<ExecSessionRequest>,
) -> Result<ExecSessionStream, Status> {
    let first = stream
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("exec session stream requires start metadata"))?;
    let start = match first.payload {
        Some(exec_session_request::Payload::Start(start)) => {
            operon_core::ExecSessionStart::try_from(start).map_err(Status::invalid_argument)?
        }
        Some(_) => {
            return Err(Status::invalid_argument(
                "exec session first message must be start metadata",
            ))
        }
        None => {
            return Err(Status::invalid_argument(
                "exec session start is missing payload",
            ))
        }
    };
    let SessionHandle {
        control_tx,
        mut event_rx,
    } = start_exec_session(&state, start)?;

    let input_control_tx = control_tx.clone();
    tokio::spawn(async move {
        while let Ok(Some(message)) = stream.message().await {
            match message.payload {
                Some(exec_session_request::Payload::Input(input)) => {
                    let _ = input_control_tx.send(SessionControl::Input(input.data));
                }
                Some(exec_session_request::Payload::Resize(resize)) => {
                    if let Ok(size) = pty_size(resize.rows, resize.cols) {
                        let _ = input_control_tx.send(SessionControl::Resize(size));
                    }
                }
                Some(exec_session_request::Payload::Start(_)) | None => {}
            }
        }
        let _ = input_control_tx.send(SessionControl::CloseInput);
    });

    let output = async_stream::stream! {
        let mut session_guard = SessionStreamGuard::new(control_tx);
        while let Some(event) = event_rx.recv().await {
            let terminal = matches!(event.event, Some(exec_session_event::Event::Exit(_)));
            if terminal {
                session_guard.disarm();
            }
            yield Ok::<_, Status>(event);
            if terminal {
                break;
            }
        }
    };
    Ok(Box::pin(output))
}

struct SessionHandle {
    control_tx: std_mpsc::Sender<SessionControl>,
    event_rx: mpsc::UnboundedReceiver<GrpcExecSessionEvent>,
}

struct SessionStreamGuard {
    control_tx: std_mpsc::Sender<SessionControl>,
    terminate_on_drop: bool,
}

impl SessionStreamGuard {
    fn new(control_tx: std_mpsc::Sender<SessionControl>) -> Self {
        Self {
            control_tx,
            terminate_on_drop: true,
        }
    }

    fn disarm(&mut self) {
        self.terminate_on_drop = false;
    }
}

impl Drop for SessionStreamGuard {
    fn drop(&mut self) {
        if self.terminate_on_drop {
            let _ = self.control_tx.send(SessionControl::Terminate);
        }
    }
}

fn start_exec_session(state: &AppState, start: ExecSessionStart) -> Result<SessionHandle, Status> {
    ensure_exec_session_platform_supported()?;
    if start.command.is_empty() && start.argv.is_empty() {
        return Err(Status::invalid_argument(
            "exec session requires command or argv",
        ));
    }
    let cwd_virtual = start.cwd.clone().unwrap_or_else(|| "/".to_string());
    let decision = authorize_exec_session_decision(
        &state.policy.subject,
        &state.policy.exec,
        &cwd_virtual,
        start.timeout_secs,
    );
    if !decision.allowed {
        record_policy_decision(state, &decision);
        return Err(status_from_error(decision.runtime_error()));
    }
    let secret_env = match resolve_exec_secrets_decision(
        &state.policy.subject,
        &state.policy.exec,
        &state.secrets,
        &start.secrets,
    ) {
        Ok(secret_env) => secret_env,
        Err(decision) => {
            record_policy_decision(state, &decision);
            return Err(status_from_error(decision.runtime_error()));
        }
    };
    let cwd = match resolve_existing_workspace_path(&state.workspace, &cwd_virtual) {
        Ok(path) => path,
        Err(error) => {
            record_audit_capability(
                state,
                "exec:default",
                "session",
                &cwd_virtual,
                false,
                &error.1,
            );
            return Err(status_from_error(error));
        }
    };
    let env = exec_environment(&state.policy.exec, secret_env);
    let size = pty_size(start.rows as u32, start.cols as u32)?;
    let exec_id = format!("exec-{}", state.next_exec_id.fetch_add(1, Ordering::SeqCst));
    let command_label = if start.argv.is_empty() {
        start.command.clone()
    } else {
        start.argv.join(" ")
    };
    let record = ExecRecord {
        id: exec_id.clone(),
        node_id: state.node.id.clone(),
        command: command_label,
        cwd: cwd_virtual,
        status: ExecStatus::Running,
        exit_code: None,
        log_count: 0,
        logs_truncated: false,
    };
    let (exec_event_tx, _) = tokio::sync::broadcast::channel(32);
    let (log_tx, _) = tokio::sync::broadcast::channel(1024);
    lock(&state.execs, "exec map")?.insert(exec_id.clone(), record.clone());
    lock(&state.exec_logs, "exec log")?.insert(exec_id.clone(), ExecLogBuffer::default());
    lock(&state.exec_events, "exec event")?.insert(exec_id.clone(), exec_event_tx);
    lock(&state.exec_log_events, "exec log event")?.insert(exec_id.clone(), log_tx);
    record_audit_capability(state, "exec:default", "session", &exec_id, true, "allowed");
    for secret in &start.secrets {
        record_audit_capability(state, "secret:default", "use", secret, true, "allowed");
    }

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    lock(&state.exec_cancel, "exec cancel")?.insert(exec_id.clone(), cancel_tx);
    let (control_tx, control_rx) = std_mpsc::channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let cancel_control_tx = control_tx.clone();
    tokio::spawn(async move {
        let _ = cancel_rx.await;
        let _ = cancel_control_tx.send(SessionControl::Terminate);
    });

    let completion = ExecCompletion {
        audit: state.audit.clone(),
        execs: state.execs.clone(),
        logs: state.exec_logs.clone(),
        events: state.exec_events.clone(),
        log_events: state.exec_log_events.clone(),
        cancels: state.exec_cancel.clone(),
        stdin: state.exec_stdin.clone(),
        store_writer: state.store_writer.clone(),
        exec_id: exec_id.clone(),
        subject: state.policy.subject.clone(),
        node_id: state.node.id.clone(),
        audit_context: current_request_context(),
    };
    let task = SessionTask {
        completion,
        execs: state.execs.clone(),
        logs: state.exec_logs.clone(),
        log_events: state.exec_log_events.clone(),
        store_writer: state.store_writer.clone(),
        exec_id,
        command: start.command,
        argv: start.argv,
        cwd,
        timeout_secs: start
            .timeout_secs
            .unwrap_or(state.policy.exec.default_timeout_secs),
        env,
        size,
        control_rx,
        event_tx,
    };
    task::spawn_blocking(move || run_session_task(task));
    Ok(SessionHandle {
        control_tx,
        event_rx,
    })
}

fn ensure_exec_session_platform_supported() -> Result<(), Status> {
    Ok(())
}

struct SessionTask {
    completion: ExecCompletion,
    execs: Arc<Mutex<std::collections::BTreeMap<String, ExecRecord>>>,
    logs: Arc<Mutex<std::collections::BTreeMap<String, ExecLogBuffer>>>,
    log_events: Arc<Mutex<std::collections::BTreeMap<String, ExecLogSender>>>,
    store_writer: operon_store::StoreWriter,
    exec_id: String,
    command: String,
    argv: Vec<String>,
    cwd: PathBuf,
    timeout_secs: u64,
    env: std::collections::BTreeMap<String, String>,
    size: PtySize,
    control_rx: std_mpsc::Receiver<SessionControl>,
    event_tx: mpsc::UnboundedSender<GrpcExecSessionEvent>,
}

fn run_session_task(task: SessionTask) {
    let result = run_session_task_inner(&task);
    let (status, exit_code) = result.unwrap_or_else(|error| {
        append_exec_log(
            &task.execs,
            &task.logs,
            &task.log_events,
            &task.store_writer,
            &task.exec_id,
            ExecLog {
                stream: "stderr".to_string(),
                data: format!("failed to run exec session: {error:#}").into_bytes(),
                sequence: 0,
            },
        );
        (ExecStatus::Failed, None)
    });
    finish_exec(&task.completion, status.clone(), exit_code);
    let _ = task.event_tx.send(
        ExecSessionEvent::Exit(ExecSessionExit {
            exec_id: task.exec_id.clone(),
            status,
            exit_code,
        })
        .into(),
    );
}

fn run_session_task_inner(task: &SessionTask) -> anyhow::Result<(ExecStatus, Option<i32>)> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(task.size)?;
    let portable_pty::PtyPair { master, slave } = pair;
    let mut command = session_command(task);
    command.cwd(&task.cwd);
    command.env_clear();
    for (key, value) in &task.env {
        command.env(key, value);
    }
    let mut child = slave.spawn_command(command)?;
    drop(slave);
    let mut killer = child.clone_killer();
    let mut reader = master.try_clone_reader()?;
    let mut writer = None;
    let output_sink = OutputSink {
        execs: task.execs.clone(),
        logs: task.logs.clone(),
        log_events: task.log_events.clone(),
        store_writer: task.store_writer.clone(),
        exec_id: task.exec_id.clone(),
        event_tx: task.event_tx.clone(),
    };
    let _reader_thread = thread::spawn(move || read_session_output(&mut reader, output_sink));
    let (wait_tx, wait_rx) = std_mpsc::channel();
    thread::spawn(move || {
        let status = child.wait();
        let _ = wait_tx.send(status);
    });
    let _ = task.event_tx.send(
        ExecSessionEvent::Started(ExecSessionStarted {
            exec_id: task.exec_id.clone(),
        })
        .into(),
    );

    let deadline = Instant::now() + Duration::from_secs(task.timeout_secs);
    let mut forced_status = None;
    loop {
        if let Ok(status) = wait_rx.try_recv() {
            return Ok(exec_status_from_pty(status, forced_status));
        }
        if forced_status.is_none() && Instant::now() >= deadline {
            let _ = killer.kill();
            forced_status = Some(ExecStatus::TimedOut);
        }
        match task.control_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(SessionControl::Input(data)) => {
                if writer.is_none() {
                    match master.take_writer() {
                        Ok(master_writer) => writer = Some(master_writer),
                        Err(_) => {
                            forced_status.get_or_insert(ExecStatus::Failed);
                            continue;
                        }
                    }
                }
                if let Some(writer) = writer.as_mut() {
                    if writer.write_all(&data).is_err() {
                        forced_status.get_or_insert(ExecStatus::Failed);
                    }
                    let _ = writer.flush();
                }
            }
            Ok(SessionControl::Resize(size)) => {
                let _ = master.resize(size);
            }
            Ok(SessionControl::CloseInput) => {
                if writer.take().is_none() {
                    let _ = master.take_writer();
                }
            }
            Ok(SessionControl::Terminate) | Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                if forced_status.is_none() {
                    let _ = killer.kill();
                    forced_status = Some(ExecStatus::Cancelled);
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn session_command(task: &SessionTask) -> CommandBuilder {
    if task.argv.is_empty() {
        let mut command = CommandBuilder::new(session_shell_program());
        command.arg(session_shell_arg());
        command.arg(&task.command);
        command
    } else {
        let mut command = CommandBuilder::new(&task.argv[0]);
        command.args(&task.argv[1..]);
        command
    }
}

#[cfg(windows)]
fn session_shell_program() -> &'static str {
    "cmd.exe"
}

#[cfg(not(windows))]
fn session_shell_program() -> &'static str {
    "/bin/sh"
}

#[cfg(windows)]
fn session_shell_arg() -> &'static str {
    "/C"
}

#[cfg(not(windows))]
fn session_shell_arg() -> &'static str {
    "-c"
}

#[derive(Clone)]
struct OutputSink {
    execs: Arc<Mutex<std::collections::BTreeMap<String, ExecRecord>>>,
    logs: Arc<Mutex<std::collections::BTreeMap<String, ExecLogBuffer>>>,
    log_events: Arc<Mutex<std::collections::BTreeMap<String, ExecLogSender>>>,
    store_writer: operon_store::StoreWriter,
    exec_id: String,
    event_tx: mpsc::UnboundedSender<GrpcExecSessionEvent>,
}

fn read_session_output(reader: &mut Box<dyn Read + Send>, sink: OutputSink) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let data = buffer[..count].to_vec();
                append_exec_log(
                    &sink.execs,
                    &sink.logs,
                    &sink.log_events,
                    &sink.store_writer,
                    &sink.exec_id,
                    ExecLog {
                        stream: "stdout".to_string(),
                        data: data.clone(),
                        sequence: 0,
                    },
                );
                let _ = sink.event_tx.send(
                    ExecSessionEvent::Output(ExecSessionOutput {
                        exec_id: sink.exec_id.clone(),
                        data,
                    })
                    .into(),
                );
            }
            Err(error) => {
                append_exec_log(
                    &sink.execs,
                    &sink.logs,
                    &sink.log_events,
                    &sink.store_writer,
                    &sink.exec_id,
                    ExecLog {
                        stream: "stderr".to_string(),
                        data: format!("failed to read exec session output: {error}").into_bytes(),
                        sequence: 0,
                    },
                );
                break;
            }
        }
    }
}

fn exec_status_from_pty(
    status: std::io::Result<portable_pty::ExitStatus>,
    forced_status: Option<ExecStatus>,
) -> (ExecStatus, Option<i32>) {
    if let Some(status) = forced_status {
        return (status, None);
    }
    match status {
        Ok(status) if status.success() => (ExecStatus::Succeeded, Some(status.exit_code() as i32)),
        Ok(status) => (ExecStatus::Failed, Some(status.exit_code() as i32)),
        Err(_) => (ExecStatus::Failed, None),
    }
}

fn pty_size(rows: u32, cols: u32) -> Result<PtySize, Status> {
    let rows = if rows == 0 { 24 } else { rows };
    let cols = if cols == 0 { 80 } else { cols };
    let rows = u16::try_from(rows)
        .map_err(|_| Status::invalid_argument("exec session rows is out of range"))?;
    let cols = u16::try_from(cols)
        .map_err(|_| Status::invalid_argument("exec session cols is out of range"))?;
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_shell_invocation_matches_platform() {
        #[cfg(windows)]
        {
            assert_eq!(session_shell_program(), "cmd.exe");
            assert_eq!(session_shell_arg(), "/C");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(session_shell_program(), "/bin/sh");
            assert_eq!(session_shell_arg(), "-c");
        }
    }

    #[test]
    fn pty_size_defaults_zero_dimensions() {
        let size = pty_size(0, 0).expect("default size");

        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn exec_session_stream_guard_terminates_on_drop_before_exit() {
        let (tx, rx) = std_mpsc::channel();
        drop(SessionStreamGuard::new(tx));

        assert!(matches!(rx.recv(), Ok(SessionControl::Terminate)));
    }

    #[test]
    fn exec_session_stream_guard_disarms_after_terminal_exit() {
        let (tx, rx) = std_mpsc::channel();
        let mut guard = SessionStreamGuard::new(tx);
        guard.disarm();
        drop(guard);

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn exec_session_platform_is_supported() {
        ensure_exec_session_platform_supported().expect("exec session platform supported");
    }

    #[test]
    fn exec_session_portable_pty_smoke_outputs_and_exits() {
        fn run_candidate(
            name: &str,
            command: CommandBuilder,
            input: Option<&'static [u8]>,
        ) -> Result<String, String> {
            let pty_system = native_pty_system();
            let pair = pty_system
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| format!("{name}: open pty failed: {error:#}"))?;
            let portable_pty::PtyPair { master, slave } = pair;
            master
                .resize(PtySize {
                    rows: 30,
                    cols: 100,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| format!("{name}: resize pty failed: {error:#}"))?;

            let mut child = slave
                .spawn_command(command)
                .map_err(|error| format!("{name}: spawn pty command failed: {error:#}"))?;
            drop(slave);
            let mut killer = child.clone_killer();
            let mut reader = master
                .try_clone_reader()
                .map_err(|error| format!("{name}: clone pty reader failed: {error:#}"))?;
            let (tx, rx) = std_mpsc::channel();
            thread::spawn(move || {
                let mut buffer = [0_u8; 1024];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            let output = String::from_utf8_lossy(&buffer[..count]).to_string();
                            let _ = tx.send(output);
                        }
                        Err(_) => break,
                    }
                }
            });

            if let Some(input) = input {
                let mut writer = master
                    .take_writer()
                    .map_err(|error| format!("{name}: take pty writer failed: {error:#}"))?;
                writer
                    .write_all(input)
                    .map_err(|error| format!("{name}: write pty input failed: {error:#}"))?;
                writer
                    .flush()
                    .map_err(|error| format!("{name}: flush pty input failed: {error:#}"))?;
                drop(writer);
            }

            let output_deadline = Instant::now() + Duration::from_secs(10);
            let mut output = String::new();
            while !output.contains("operon-pty-smoke") {
                let remaining = output_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    let _ = killer.kill();
                    return Err(format!("{name}: pty output timed out; output={:?}", output));
                }
                match rx.recv_timeout(remaining) {
                    Ok(chunk) => output.push_str(&chunk),
                    Err(error) => {
                        let _ = killer.kill();
                        return Err(format!(
                            "{name}: pty output timed out: {error}; output={:?}",
                            output
                        ));
                    }
                }
            }

            if input.is_none() {
                let writer = master
                    .take_writer()
                    .map_err(|error| format!("{name}: take pty writer failed: {error:#}"))?;
                drop(writer);
            }

            let (wait_tx, wait_rx) = std_mpsc::channel();
            thread::spawn(move || {
                let _ = wait_tx.send(child.wait());
            });

            let status = match wait_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(status) => {
                    status.map_err(|error| format!("{name}: wait pty child failed: {error}"))?
                }
                Err(error) => {
                    let _ = killer.kill();
                    return Err(format!(
                        "{name}: pty child wait timed out: {error}; output={:?}",
                        output
                    ));
                }
            };
            if !status.success() {
                return Err(format!(
                    "{name}: pty child exited unsuccessfully: {:?}; output={:?}",
                    status, output
                ));
            }

            Ok(output)
        }

        let mut failures = Vec::new();

        #[cfg(windows)]
        let candidates = {
            use std::ffi::OsString;

            let mut shell_command = CommandBuilder::new(session_shell_program());
            shell_command.arg(session_shell_arg());
            shell_command.arg("echo operon-pty-smoke");

            let mut quiet_cmd = CommandBuilder::new("cmd.exe");
            quiet_cmd.args(["/D", "/Q", "/C", "echo operon-pty-smoke"]);

            let mut powershell = CommandBuilder::new("powershell.exe");
            powershell.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output operon-pty-smoke",
            ]);

            let default_shell = CommandBuilder::new_default_prog();

            vec![
                ("cmd-shell-command", shell_command, None),
                ("cmd-quiet-argv", quiet_cmd, None),
                ("powershell-command", powershell, None),
                (
                    "default-shell-input",
                    default_shell,
                    Some(b"echo operon-pty-smoke\r\nexit\r\n" as &'static [u8]),
                ),
                (
                    "cmd-from-argv-input",
                    CommandBuilder::from_argv(vec![OsString::from("cmd.exe")]),
                    Some(b"echo operon-pty-smoke\r\nexit\r\n" as &'static [u8]),
                ),
            ]
        };

        #[cfg(not(windows))]
        let candidates = {
            let mut command = CommandBuilder::new(session_shell_program());
            command.arg(session_shell_arg());
            command.arg("echo operon-pty-smoke");
            vec![("unix-shell-command", command, None)]
        };

        for (name, command, input) in candidates {
            match run_candidate(name, command, input) {
                Ok(output) => {
                    assert!(
                        output.contains("operon-pty-smoke"),
                        "{name}: output did not include marker: {output:?}"
                    );
                    return;
                }
                Err(error) => failures.push(error),
            }
        }

        panic!(
            "all portable-pty smoke candidates failed:\n{}",
            failures.join("\n")
        );
    }
}
