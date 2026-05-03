use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{atomic::Ordering, Arc, Mutex},
};

use operon_core::{
    AuditEvent, ExecEvent, ExecLog, ExecLogList, ExecRecord, ExecRunRequest, ExecStatus,
};
use operon_fs::resolve_existing_workspace_path;
use operon_process::{authorize_exec_decision, exec_environment, resolve_exec_secrets_decision};
use operon_protocol::runtime::v1::{
    exec_log_stream_event, ExecLogComplete, ExecLogEntry, ExecLogSnapshot, ExecLogStreamEvent,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command as TokioCommand},
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
    time,
};
use tonic::Status;

use crate::{
    audit::{
        append_store_record, current_request_context, now_ms, push_audit_event,
        record_audit_capability, record_policy_decision,
    },
    grpc_status::status_from_error,
    locks::lock,
    state::{
        AppState, ExecCompletion, ExecLogBuffer, ExecLogSender, ExecTask,
        MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS, MAX_IN_MEMORY_EXEC_LOGS,
    },
    AUDIT_CONTEXT,
};
pub(crate) fn start_exec(state: &AppState, request: ExecRunRequest) -> Result<ExecRecord, Status> {
    if request.command.is_empty() && request.argv.is_empty() {
        return Err(Status::invalid_argument(
            "exec run requires command or argv",
        ));
    }
    let cwd_virtual = request.cwd.clone().unwrap_or_else(|| "/".to_string());
    let decision = authorize_exec_decision(
        &state.policy.subject,
        &state.policy.exec,
        &cwd_virtual,
        request.timeout_secs,
    );
    if !decision.allowed {
        record_policy_decision(state, &decision);
        return Err(status_from_error(decision.runtime_error()));
    }
    let secret_env = match resolve_exec_secrets_decision(
        &state.policy.subject,
        &state.policy.exec,
        &state.secrets,
        &request.secrets,
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
            record_audit_capability(state, "exec:default", "run", &cwd_virtual, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let env = exec_environment(&state.policy.exec, secret_env);

    let exec_id = format!("exec-{}", state.next_exec_id.fetch_add(1, Ordering::SeqCst));
    let record = ExecRecord {
        id: exec_id.clone(),
        node_id: state.node.id.clone(),
        command: request.command.clone(),
        cwd: cwd_virtual,
        status: ExecStatus::Running,
        exit_code: None,
        log_count: 0,
        logs_truncated: false,
    };
    let (event_tx, _) = broadcast::channel(32);
    let (log_tx, _) = broadcast::channel(1024);
    lock(&state.execs, "exec map")?.insert(exec_id.clone(), record.clone());
    lock(&state.exec_logs, "exec log")?.insert(exec_id.clone(), ExecLogBuffer::default());
    lock(&state.exec_events, "exec event")?.insert(exec_id.clone(), event_tx);
    lock(&state.exec_log_events, "exec log event")?.insert(exec_id.clone(), log_tx);
    record_audit_capability(state, "exec:default", "run", &exec_id, true, "allowed");
    for secret in &request.secrets {
        record_audit_capability(state, "secret:default", "use", secret, true, "allowed");
    }

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    lock(&state.exec_cancel, "exec cancel")?.insert(exec_id.clone(), cancel_tx);
    lock(&state.exec_stdin, "exec stdin")?.insert(exec_id.clone(), stdin_tx);

    let audit = state.audit.clone();
    let execs = state.execs.clone();
    let logs = state.exec_logs.clone();
    let events = state.exec_events.clone();
    let log_events = state.exec_log_events.clone();
    let cancels = state.exec_cancel.clone();
    let stdin = state.exec_stdin.clone();
    let store_writer = state.store_writer.clone();
    let command = request.command;
    let argv = request.argv;
    let timeout_secs = request
        .timeout_secs
        .unwrap_or(state.policy.exec.default_timeout_secs);
    let audit_context = current_request_context();
    let subject = state.policy.subject.clone();
    let node_id = state.node.id.clone();

    tokio::spawn(async move {
        let context = audit_context.clone();
        AUDIT_CONTEXT
            .scope(context, async move {
                run_exec_task(ExecTask {
                    audit,
                    execs,
                    logs,
                    events,
                    log_events,
                    cancels,
                    stdin,
                    store_writer,
                    exec_id,
                    command,
                    argv,
                    cwd,
                    timeout_secs,
                    env,
                    subject,
                    node_id,
                    audit_context,
                    cancel_rx,
                    stdin_rx,
                })
                .await;
            })
            .await;
    });

    Ok(record)
}

pub(crate) fn get_exec_record(state: &AppState, exec_id: &str) -> Result<ExecRecord, Status> {
    lock(&state.execs, "exec map")?
        .get(exec_id)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("exec `{exec_id}` not found")))
}

pub(crate) async fn run_exec_task(task: ExecTask) {
    let completion = ExecCompletion {
        audit: task.audit.clone(),
        execs: task.execs.clone(),
        logs: task.logs.clone(),
        events: task.events.clone(),
        log_events: task.log_events.clone(),
        cancels: task.cancels.clone(),
        stdin: task.stdin.clone(),
        store_writer: task.store_writer.clone(),
        exec_id: task.exec_id.clone(),
        subject: task.subject.clone(),
        node_id: task.node_id.clone(),
        audit_context: task.audit_context.clone(),
    };
    let mut child = match build_exec_command(&task).spawn() {
        Ok(child) => child,
        Err(error) => {
            append_exec_log(
                &task.execs,
                &task.logs,
                &task.log_events,
                &task.store_writer,
                &task.exec_id,
                ExecLog {
                    stream: "stderr".to_string(),
                    data: format!("failed to spawn command: {error}").into_bytes(),
                    sequence: 0,
                },
            );
            finish_exec(&completion, ExecStatus::Failed, None);
            return;
        }
    };

    if let Some(stdin) = child.stdin.take() {
        tokio::spawn(pump_exec_stdin(task.stdin_rx, stdin));
    }
    let mut capture_tasks = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        capture_tasks.push(tokio::spawn(capture_exec_stream(
            task.execs.clone(),
            task.logs.clone(),
            task.log_events.clone(),
            task.store_writer.clone(),
            task.exec_id.clone(),
            "stdout",
            stdout,
        )));
    }
    if let Some(stderr) = child.stderr.take() {
        capture_tasks.push(tokio::spawn(capture_exec_stream(
            task.execs.clone(),
            task.logs.clone(),
            task.log_events.clone(),
            task.store_writer.clone(),
            task.exec_id.clone(),
            "stderr",
            stderr,
        )));
    }

    let (exec_status, exit_code) = tokio::select! {
        status = child.wait() => exec_status_from_wait(status, &task.execs, &task.logs, &task.log_events, &task.store_writer, &task.exec_id),
        _ = task.cancel_rx => {
            terminate_child(&mut child).await;
            (ExecStatus::Cancelled, None)
        }
        _ = time::sleep(std::time::Duration::from_secs(task.timeout_secs)) => {
            terminate_child(&mut child).await;
            (ExecStatus::TimedOut, None)
        }
    };

    wait_for_capture_tasks(capture_tasks).await;
    finish_exec(&completion, exec_status, exit_code);
}

fn exec_status_from_wait(
    status: std::io::Result<std::process::ExitStatus>,
    execs: &Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    log_events: &Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    store_writer: &operon_store::StoreWriter,
    exec_id: &str,
) -> (ExecStatus, Option<i32>) {
    match status {
        Ok(status) => {
            let exec_status = if status.success() {
                ExecStatus::Succeeded
            } else {
                ExecStatus::Failed
            };
            (exec_status, status.code())
        }
        Err(error) => {
            append_exec_log(
                execs,
                logs,
                log_events,
                store_writer,
                exec_id,
                ExecLog {
                    stream: "stderr".to_string(),
                    data: error.to_string().into_bytes(),
                    sequence: 0,
                },
            );
            (ExecStatus::Failed, None)
        }
    }
}

fn build_exec_command(task: &ExecTask) -> TokioCommand {
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

pub(crate) async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        terminate_child_process_group(child).await
    }

    #[cfg(not(unix))]
    {
        terminate_direct_child(child).await
    }
}

#[cfg(unix)]
pub(crate) async fn terminate_child_process_group(child: &mut Child) {
    let Some(pid) = child.id().map(|pid| pid as libc::pid_t) else {
        if let Err(error) = child.wait().await {
            tracing::warn!("failed to wait for finished exec process: {error}");
        }
        return;
    };

    signal_process_group(pid, libc::SIGTERM);
    match time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(Ok(_)) => return,
        Ok(Err(error)) => {
            tracing::warn!("failed to wait for terminated exec process group: {error}");
            return;
        }
        Err(_) => {}
    }

    signal_process_group(pid, libc::SIGKILL);
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed exec process group: {error}");
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: libc::pid_t, signal: libc::c_int) {
    let result = unsafe { libc::kill(-pgid, signal) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!("failed to signal exec process group {pgid}: {error}");
        }
    }
}

#[cfg(not(unix))]
pub(crate) async fn terminate_direct_child(child: &mut Child) {
    if let Err(error) = child.start_kill() {
        tracing::warn!("failed to kill exec process: {error}");
    }
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed exec process: {error}");
    }
}

pub(crate) async fn wait_for_capture_tasks(capture_tasks: Vec<JoinHandle<()>>) {
    for task in capture_tasks {
        if let Err(error) = task.await {
            tracing::warn!("exec stream capture task failed: {error}");
        }
    }
}

pub(crate) async fn capture_exec_stream<R>(
    execs: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    store_writer: operon_store::StoreWriter,
    exec_id: String,
    stream: &'static str,
    mut reader: R,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => append_exec_log(
                &execs,
                &logs,
                &log_events,
                &store_writer,
                &exec_id,
                ExecLog {
                    stream: stream.to_string(),
                    data: buffer[..count].to_vec(),
                    sequence: 0,
                },
            ),
            Err(error) => {
                append_exec_log(
                    &execs,
                    &logs,
                    &log_events,
                    &store_writer,
                    &exec_id,
                    ExecLog {
                        stream: "stderr".to_string(),
                        data: format!("failed to read {stream}: {error}").into_bytes(),
                        sequence: 0,
                    },
                );
                break;
            }
        }
    }
}

pub(crate) async fn pump_exec_stdin(
    mut receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(chunk) = receiver.recv().await {
        if stdin.write_all(&chunk).await.is_err() {
            break;
        }
    }
}

pub(crate) fn exec_event_from_record(record: &ExecRecord) -> ExecEvent {
    ExecEvent {
        exec_id: record.id.clone(),
        status: record.status.clone(),
        exit_code: record.exit_code,
        log_count: record.log_count,
        logs_truncated: record.logs_truncated,
    }
}

pub(crate) fn exec_log_list(state: &AppState, exec_id: &str) -> Result<ExecLogList, Status> {
    let logs = lock(&state.exec_logs, "exec log")?;
    let Some(buffer) = logs.get(exec_id) else {
        return Ok(ExecLogList {
            exec_id: exec_id.to_string(),
            logs: Vec::new(),
            truncated: false,
            dropped_log_count: 0,
        });
    };
    Ok(ExecLogList {
        exec_id: exec_id.to_string(),
        logs: buffer.logs.iter().cloned().collect(),
        truncated: buffer.dropped_log_count > 0,
        dropped_log_count: buffer.dropped_log_count,
    })
}

pub(crate) fn exec_log_snapshot(
    state: &AppState,
    exec_id: &str,
) -> Result<(ExecLogSnapshot, u64), Status> {
    let logs = lock(&state.exec_logs, "exec log")?;
    let Some(buffer) = logs.get(exec_id) else {
        return Ok((
            ExecLogSnapshot {
                exec_id: exec_id.to_string(),
                logs: Vec::new(),
                truncated: false,
                dropped_log_count: 0,
                next_sequence: 0,
            },
            0,
        ));
    };
    Ok((
        ExecLogSnapshot {
            exec_id: exec_id.to_string(),
            logs: buffer.logs.iter().cloned().map(Into::into).collect(),
            truncated: buffer.dropped_log_count > 0,
            dropped_log_count: buffer.dropped_log_count,
            next_sequence: buffer.next_sequence,
        },
        buffer.next_sequence,
    ))
}

pub(crate) fn exec_log_snapshot_event(snapshot: ExecLogSnapshot) -> ExecLogStreamEvent {
    ExecLogStreamEvent {
        event: Some(exec_log_stream_event::Event::Snapshot(snapshot)),
    }
}

pub(crate) fn exec_log_entry_event(exec_id: &str, log: ExecLog) -> ExecLogStreamEvent {
    ExecLogStreamEvent {
        event: Some(exec_log_stream_event::Event::Entry(ExecLogEntry {
            exec_id: exec_id.to_string(),
            log: Some(log.into()),
        })),
    }
}

pub(crate) fn exec_log_complete(
    state: &AppState,
    exec_id: &str,
) -> Result<ExecLogComplete, Status> {
    let record = get_exec_record(state, exec_id)?;
    let logs = exec_log_list(state, exec_id)?;
    let event: operon_protocol::runtime::v1::ExecEvent = exec_event_from_record(&record).into();
    Ok(ExecLogComplete {
        exec_id: exec_id.to_string(),
        status: event.status,
        exit_code: event.exit_code,
        log_count: event.log_count,
        logs_truncated: event.logs_truncated,
        truncated: logs.truncated,
        dropped_log_count: logs.dropped_log_count,
    })
}

pub(crate) fn exec_log_complete_event(complete: ExecLogComplete) -> ExecLogStreamEvent {
    ExecLogStreamEvent {
        event: Some(exec_log_stream_event::Event::Complete(complete)),
    }
}

pub(crate) fn append_exec_log(
    execs: &Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    log_events: &Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    store_writer: &operon_store::StoreWriter,
    exec_id: &str,
    mut log: ExecLog,
) {
    let (log_count, logs_truncated, dropped_log_count, log) = {
        let Ok(mut buffers) = logs.lock() else {
            tracing::error!("exec log mutex poisoned");
            return;
        };
        let buffer = buffers.entry(exec_id.to_string()).or_default();
        log.sequence = buffer.next_sequence;
        buffer.next_sequence = buffer.next_sequence.saturating_add(1);
        buffer.logs.push_back(log);
        while buffer.logs.len() > MAX_IN_MEMORY_EXEC_LOGS {
            buffer.logs.pop_front();
            buffer.dropped_log_count = buffer.dropped_log_count.saturating_add(1);
        }
        let Some(log) = buffer.logs.back().cloned() else {
            tracing::error!("exec log buffer unexpectedly empty after append");
            return;
        };
        (
            buffer.next_sequence,
            buffer.dropped_log_count > 0,
            buffer.dropped_log_count,
            log,
        )
    };

    if let Ok(mut execs) = execs.lock() {
        if let Some(record) = execs.get_mut(exec_id) {
            record.log_count = log_count;
            record.logs_truncated = logs_truncated;
        }
    } else {
        tracing::error!("exec map mutex poisoned");
    }
    match log_events.lock() {
        Ok(log_events) => {
            if let Some(sender) = log_events.get(exec_id) {
                let _ = sender.send(log.clone());
            }
        }
        Err(_) => tracing::error!("exec log event mutex poisoned"),
    }
    if let Err(error) = append_store_record(
        store_writer,
        &serde_json::json!({
            "kind": "exec_log",
            "exec_id": exec_id,
            "log": log,
            "dropped_log_count": dropped_log_count,
        }),
    ) {
        tracing::warn!("failed to persist exec log: {error:#}");
    }
}

pub(crate) fn finish_exec(completion: &ExecCompletion, status: ExecStatus, exit_code: Option<i32>) {
    if let Ok(mut cancels) = completion.cancels.lock() {
        cancels.remove(&completion.exec_id);
    } else {
        tracing::error!("exec cancel mutex poisoned");
    }
    if let Ok(mut stdin) = completion.stdin.lock() {
        stdin.remove(&completion.exec_id);
    } else {
        tracing::error!("exec stdin mutex poisoned");
    }

    let terminal = {
        let Ok(mut execs) = completion.execs.lock() else {
            tracing::error!("exec map mutex poisoned");
            cleanup_finished_exec_runtime(completion);
            return;
        };
        if let Some(record) = execs.get_mut(&completion.exec_id) {
            record.status = status;
            record.exit_code = exit_code;
            let event = exec_event_from_record(record);
            Some((event, record.clone()))
        } else {
            None
        }
    };
    if let Some((event, record)) = terminal {
        if let Err(error) = append_store_record(
            &completion.store_writer,
            &serde_json::json!({
                "kind": "exec",
                "record": record,
            }),
        ) {
            tracing::warn!("failed to persist exec record: {error:#}");
        }
        record_exec_completion_audit(completion, &record);
        match completion.events.lock() {
            Ok(events) => {
                if let Some(sender) = events.get(&completion.exec_id) {
                    let _ = sender.send(event);
                }
            }
            Err(_) => tracing::error!("exec event mutex poisoned"),
        }
    }
    cleanup_finished_exec_runtime(completion);
}

fn record_exec_completion_audit(completion: &ExecCompletion, record: &ExecRecord) {
    let reason = match record.exit_code {
        Some(code) => format!(
            "status={} exit_code={code}",
            operon_protocol::format_exec_status(&record.status)
        ),
        None => format!(
            "status={}",
            operon_protocol::format_exec_status(&record.status)
        ),
    };
    push_audit_event(
        &completion.audit,
        &completion.store_writer,
        AuditEvent {
            subject: completion.subject.clone(),
            timestamp_ms: now_ms(),
            node_id: completion.node_id.clone(),
            capability: "exec:default".to_string(),
            action: "finish".to_string(),
            resource: completion.exec_id.clone(),
            allowed: true,
            reason,
            run_id: completion.audit_context.run_id.clone(),
            step_id: completion.audit_context.step_id.clone(),
        },
    );
}

fn cleanup_finished_exec_runtime(completion: &ExecCompletion) {
    if let Ok(mut events) = completion.events.lock() {
        events.remove(&completion.exec_id);
    } else {
        tracing::error!("exec event mutex poisoned");
    }
    if let Ok(mut log_events) = completion.log_events.lock() {
        log_events.remove(&completion.exec_id);
    } else {
        tracing::error!("exec log event mutex poisoned");
    }
    prune_completed_exec_log_buffers(&completion.execs, &completion.logs);
}

pub(crate) fn next_exec_sequence(execs: &BTreeMap<String, ExecRecord>) -> u64 {
    execs
        .keys()
        .filter_map(|id| exec_sequence_number(id))
        .max()
        .unwrap_or(0)
        + 1
}

fn exec_sequence_number(exec_id: &str) -> Option<u64> {
    exec_id.strip_prefix("exec-")?.parse::<u64>().ok()
}

pub(crate) fn prune_completed_exec_log_buffers(
    execs: &Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
) {
    let Ok(execs) = execs.lock() else {
        tracing::error!("exec map mutex poisoned");
        return;
    };
    let Ok(logs_guard) = logs.lock() else {
        tracing::error!("exec log mutex poisoned");
        return;
    };
    let mut completed_log_exec_ids = logs_guard
        .keys()
        .filter(|exec_id| {
            execs
                .get(*exec_id)
                .map(|record| !matches!(record.status, ExecStatus::Running))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    drop(logs_guard);

    if completed_log_exec_ids.len() <= MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS {
        return;
    }

    completed_log_exec_ids.sort_by_key(|exec_id| exec_sequence_number(exec_id).unwrap_or(u64::MAX));
    let remove_count = completed_log_exec_ids.len() - MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS;
    drop(execs);

    match logs.lock() {
        Ok(mut logs) => {
            for exec_id in completed_log_exec_ids.into_iter().take(remove_count) {
                logs.remove(&exec_id);
            }
        }
        Err(_) => tracing::error!("exec log mutex poisoned"),
    }
}

pub(crate) fn exec_log_buffers_from_persisted_logs(
    persisted_logs: BTreeMap<String, Vec<ExecLog>>,
) -> BTreeMap<String, ExecLogBuffer> {
    let mut buffers = BTreeMap::new();
    for (exec_id, logs) in persisted_logs {
        let mut buffer = ExecLogBuffer::default();
        for log in logs {
            buffer.next_sequence = buffer.next_sequence.max(log.sequence.saturating_add(1));
            buffer.logs.push_back(log);
            while buffer.logs.len() > MAX_IN_MEMORY_EXEC_LOGS {
                buffer.logs.pop_front();
                buffer.dropped_log_count = buffer.dropped_log_count.saturating_add(1);
            }
        }
        buffers.insert(exec_id, buffer);
    }
    buffers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_shell_invocation_matches_platform() {
        #[cfg(windows)]
        {
            assert_eq!(exec_shell_program(), "cmd.exe");
            assert_eq!(exec_shell_arg(), "/C");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(exec_shell_program(), "/bin/sh");
            assert_eq!(exec_shell_arg(), "-c");
        }
    }

    #[test]
    fn persisted_exec_logs_seed_bounded_log_buffers() {
        let logs = BTreeMap::from([(
            "exec-1".to_string(),
            (0..MAX_IN_MEMORY_EXEC_LOGS + 2)
                .map(|sequence| ExecLog {
                    stream: "stdout".to_string(),
                    data: format!("line-{sequence}").into_bytes(),
                    sequence: sequence as u64,
                })
                .collect::<Vec<_>>(),
        )]);

        let buffers = exec_log_buffers_from_persisted_logs(logs);
        let buffer = buffers.get("exec-1").expect("exec log buffer");

        assert_eq!(buffer.logs.len(), MAX_IN_MEMORY_EXEC_LOGS);
        assert_eq!(buffer.logs.front().expect("first retained").sequence, 2);
        assert_eq!(buffer.next_sequence, (MAX_IN_MEMORY_EXEC_LOGS + 2) as u64);
        assert_eq!(buffer.dropped_log_count, 2);
    }
}
