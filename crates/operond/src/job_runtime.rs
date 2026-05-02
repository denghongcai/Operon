use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{atomic::Ordering, Arc, Mutex},
};

use operon_core::{AuditEvent, JobEvent, JobLog, JobLogList, JobRecord, JobRunRequest, JobStatus};
use operon_fs::resolve_existing_workspace_path;
use operon_process::{authorize_job_decision, job_environment, resolve_job_secrets_decision};
use operon_protocol::runtime::v1::{
    job_log_stream_event, JobLogComplete, JobLogEntry, JobLogSnapshot, JobLogStreamEvent,
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
        AppState, JobCompletion, JobLogBuffer, JobLogSender, JobTask,
        MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS, MAX_IN_MEMORY_JOB_LOGS,
    },
    AUDIT_CONTEXT,
};
pub(crate) fn start_job(state: &AppState, request: JobRunRequest) -> Result<JobRecord, Status> {
    let cwd_virtual = request.cwd.clone().unwrap_or_else(|| "/".to_string());
    let decision = authorize_job_decision(
        &state.policy.subject,
        &state.policy.job,
        &cwd_virtual,
        request.timeout_secs,
    );
    if !decision.allowed {
        record_policy_decision(state, &decision);
        return Err(status_from_error(decision.runtime_error()));
    }
    let secret_env = match resolve_job_secrets_decision(
        &state.policy.subject,
        &state.policy.job,
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
            record_audit_capability(state, "job:default", "run", &cwd_virtual, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let env = job_environment(&state.policy.job, secret_env);

    let job_id = format!("job-{}", state.next_job_id.fetch_add(1, Ordering::SeqCst));
    let record = JobRecord {
        id: job_id.clone(),
        node_id: state.node.id.clone(),
        command: request.command.clone(),
        cwd: cwd_virtual,
        status: JobStatus::Running,
        exit_code: None,
        log_count: 0,
        logs_truncated: false,
    };
    let (event_tx, _) = broadcast::channel(32);
    let (log_tx, _) = broadcast::channel(1024);
    lock(&state.jobs, "job map")?.insert(job_id.clone(), record.clone());
    lock(&state.job_logs, "job log")?.insert(job_id.clone(), JobLogBuffer::default());
    lock(&state.job_events, "job event")?.insert(job_id.clone(), event_tx);
    lock(&state.job_log_events, "job log event")?.insert(job_id.clone(), log_tx);
    record_audit_capability(state, "job:default", "run", &job_id, true, "allowed");
    for secret in &request.secrets {
        record_audit_capability(state, "secret:default", "use", secret, true, "allowed");
    }

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    lock(&state.job_cancel, "job cancel")?.insert(job_id.clone(), cancel_tx);
    lock(&state.job_stdin, "job stdin")?.insert(job_id.clone(), stdin_tx);

    let audit = state.audit.clone();
    let jobs = state.jobs.clone();
    let logs = state.job_logs.clone();
    let events = state.job_events.clone();
    let log_events = state.job_log_events.clone();
    let cancels = state.job_cancel.clone();
    let stdin = state.job_stdin.clone();
    let store_writer = state.store_writer.clone();
    let command = request.command;
    let argv = request.argv;
    let timeout_secs = request
        .timeout_secs
        .unwrap_or(state.policy.job.default_timeout_secs);
    let audit_context = current_request_context();
    let subject = state.policy.subject.clone();
    let node_id = state.node.id.clone();

    tokio::spawn(async move {
        let context = audit_context.clone();
        AUDIT_CONTEXT
            .scope(context, async move {
                run_job_task(JobTask {
                    audit,
                    jobs,
                    logs,
                    events,
                    log_events,
                    cancels,
                    stdin,
                    store_writer,
                    job_id,
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

pub(crate) fn get_job_record(state: &AppState, job_id: &str) -> Result<JobRecord, Status> {
    lock(&state.jobs, "job map")?
        .get(job_id)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("job `{job_id}` not found")))
}

pub(crate) async fn run_job_task(task: JobTask) {
    let completion = JobCompletion {
        audit: task.audit.clone(),
        jobs: task.jobs.clone(),
        logs: task.logs.clone(),
        events: task.events.clone(),
        log_events: task.log_events.clone(),
        cancels: task.cancels.clone(),
        stdin: task.stdin.clone(),
        store_writer: task.store_writer.clone(),
        job_id: task.job_id.clone(),
        subject: task.subject.clone(),
        node_id: task.node_id.clone(),
        audit_context: task.audit_context.clone(),
    };
    let mut child = match build_job_command(&task).spawn() {
        Ok(child) => child,
        Err(error) => {
            append_job_log(
                &task.jobs,
                &task.logs,
                &task.log_events,
                &task.store_writer,
                &task.job_id,
                JobLog {
                    stream: "stderr".to_string(),
                    data: format!("failed to spawn command: {error}").into_bytes(),
                    sequence: 0,
                },
            );
            finish_job(&completion, JobStatus::Failed, None);
            return;
        }
    };

    if let Some(stdin) = child.stdin.take() {
        tokio::spawn(pump_job_stdin(task.stdin_rx, stdin));
    }
    let mut capture_tasks = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        capture_tasks.push(tokio::spawn(capture_job_stream(
            task.jobs.clone(),
            task.logs.clone(),
            task.log_events.clone(),
            task.store_writer.clone(),
            task.job_id.clone(),
            "stdout",
            stdout,
        )));
    }
    if let Some(stderr) = child.stderr.take() {
        capture_tasks.push(tokio::spawn(capture_job_stream(
            task.jobs.clone(),
            task.logs.clone(),
            task.log_events.clone(),
            task.store_writer.clone(),
            task.job_id.clone(),
            "stderr",
            stderr,
        )));
    }

    let (job_status, exit_code) = tokio::select! {
        status = child.wait() => job_status_from_wait(status, &task.jobs, &task.logs, &task.log_events, &task.store_writer, &task.job_id),
        _ = task.cancel_rx => {
            terminate_child(&mut child).await;
            (JobStatus::Cancelled, None)
        }
        _ = time::sleep(std::time::Duration::from_secs(task.timeout_secs)) => {
            terminate_child(&mut child).await;
            (JobStatus::TimedOut, None)
        }
    };

    wait_for_capture_tasks(capture_tasks).await;
    finish_job(&completion, job_status, exit_code);
}

fn job_status_from_wait(
    status: std::io::Result<std::process::ExitStatus>,
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    log_events: &Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    store_writer: &operon_store::StoreWriter,
    job_id: &str,
) -> (JobStatus, Option<i32>) {
    match status {
        Ok(status) => {
            let job_status = if status.success() {
                JobStatus::Succeeded
            } else {
                JobStatus::Failed
            };
            (job_status, status.code())
        }
        Err(error) => {
            append_job_log(
                jobs,
                logs,
                log_events,
                store_writer,
                job_id,
                JobLog {
                    stream: "stderr".to_string(),
                    data: error.to_string().into_bytes(),
                    sequence: 0,
                },
            );
            (JobStatus::Failed, None)
        }
    }
}

fn build_job_command(task: &JobTask) -> TokioCommand {
    let mut command = if task.argv.is_empty() {
        let mut command = TokioCommand::new("/bin/sh");
        command.arg("-c").arg(&task.command);
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
    configure_job_process_group(&mut command);
    command
}

#[cfg(unix)]
fn configure_job_process_group(command: &mut TokioCommand) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_job_process_group(_command: &mut TokioCommand) {}

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
            tracing::warn!("failed to wait for finished job process: {error}");
        }
        return;
    };

    signal_process_group(pid, libc::SIGTERM);
    match time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(Ok(_)) => return,
        Ok(Err(error)) => {
            tracing::warn!("failed to wait for terminated job process group: {error}");
            return;
        }
        Err(_) => {}
    }

    signal_process_group(pid, libc::SIGKILL);
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed job process group: {error}");
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: libc::pid_t, signal: libc::c_int) {
    let result = unsafe { libc::kill(-pgid, signal) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!("failed to signal job process group {pgid}: {error}");
        }
    }
}

#[cfg(not(unix))]
pub(crate) async fn terminate_direct_child(child: &mut Child) {
    if let Err(error) = child.start_kill() {
        tracing::warn!("failed to kill job process: {error}");
    }
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed job process: {error}");
    }
}

pub(crate) async fn wait_for_capture_tasks(capture_tasks: Vec<JoinHandle<()>>) {
    for task in capture_tasks {
        if let Err(error) = task.await {
            tracing::warn!("job stream capture task failed: {error}");
        }
    }
}

pub(crate) async fn capture_job_stream<R>(
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    store_writer: operon_store::StoreWriter,
    job_id: String,
    stream: &'static str,
    mut reader: R,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => append_job_log(
                &jobs,
                &logs,
                &log_events,
                &store_writer,
                &job_id,
                JobLog {
                    stream: stream.to_string(),
                    data: buffer[..count].to_vec(),
                    sequence: 0,
                },
            ),
            Err(error) => {
                append_job_log(
                    &jobs,
                    &logs,
                    &log_events,
                    &store_writer,
                    &job_id,
                    JobLog {
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

pub(crate) async fn pump_job_stdin(
    mut receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(chunk) = receiver.recv().await {
        if stdin.write_all(&chunk).await.is_err() {
            break;
        }
    }
}

pub(crate) fn job_event_from_record(record: &JobRecord) -> JobEvent {
    JobEvent {
        job_id: record.id.clone(),
        status: record.status.clone(),
        exit_code: record.exit_code,
        log_count: record.log_count,
        logs_truncated: record.logs_truncated,
    }
}

pub(crate) fn job_log_list(state: &AppState, job_id: &str) -> Result<JobLogList, Status> {
    let logs = lock(&state.job_logs, "job log")?;
    let Some(buffer) = logs.get(job_id) else {
        return Ok(JobLogList {
            job_id: job_id.to_string(),
            logs: Vec::new(),
            truncated: false,
            dropped_log_count: 0,
        });
    };
    Ok(JobLogList {
        job_id: job_id.to_string(),
        logs: buffer.logs.iter().cloned().collect(),
        truncated: buffer.dropped_log_count > 0,
        dropped_log_count: buffer.dropped_log_count,
    })
}

pub(crate) fn job_log_snapshot(
    state: &AppState,
    job_id: &str,
) -> Result<(JobLogSnapshot, u64), Status> {
    let logs = lock(&state.job_logs, "job log")?;
    let Some(buffer) = logs.get(job_id) else {
        return Ok((
            JobLogSnapshot {
                job_id: job_id.to_string(),
                logs: Vec::new(),
                truncated: false,
                dropped_log_count: 0,
                next_sequence: 0,
            },
            0,
        ));
    };
    Ok((
        JobLogSnapshot {
            job_id: job_id.to_string(),
            logs: buffer.logs.iter().cloned().map(Into::into).collect(),
            truncated: buffer.dropped_log_count > 0,
            dropped_log_count: buffer.dropped_log_count,
            next_sequence: buffer.next_sequence,
        },
        buffer.next_sequence,
    ))
}

pub(crate) fn job_log_snapshot_event(snapshot: JobLogSnapshot) -> JobLogStreamEvent {
    JobLogStreamEvent {
        event: Some(job_log_stream_event::Event::Snapshot(snapshot)),
    }
}

pub(crate) fn job_log_entry_event(job_id: &str, log: JobLog) -> JobLogStreamEvent {
    JobLogStreamEvent {
        event: Some(job_log_stream_event::Event::Entry(JobLogEntry {
            job_id: job_id.to_string(),
            log: Some(log.into()),
        })),
    }
}

pub(crate) fn job_log_complete(state: &AppState, job_id: &str) -> Result<JobLogComplete, Status> {
    let record = get_job_record(state, job_id)?;
    let logs = job_log_list(state, job_id)?;
    let event: operon_protocol::runtime::v1::JobEvent = job_event_from_record(&record).into();
    Ok(JobLogComplete {
        job_id: job_id.to_string(),
        status: event.status,
        exit_code: event.exit_code,
        log_count: event.log_count,
        logs_truncated: event.logs_truncated,
        truncated: logs.truncated,
        dropped_log_count: logs.dropped_log_count,
    })
}

pub(crate) fn job_log_complete_event(complete: JobLogComplete) -> JobLogStreamEvent {
    JobLogStreamEvent {
        event: Some(job_log_stream_event::Event::Complete(complete)),
    }
}

pub(crate) fn append_job_log(
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    log_events: &Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    store_writer: &operon_store::StoreWriter,
    job_id: &str,
    mut log: JobLog,
) {
    let (log_count, logs_truncated, dropped_log_count, log) = {
        let Ok(mut buffers) = logs.lock() else {
            tracing::error!("job log mutex poisoned");
            return;
        };
        let buffer = buffers.entry(job_id.to_string()).or_default();
        log.sequence = buffer.next_sequence;
        buffer.next_sequence = buffer.next_sequence.saturating_add(1);
        buffer.logs.push_back(log);
        while buffer.logs.len() > MAX_IN_MEMORY_JOB_LOGS {
            buffer.logs.pop_front();
            buffer.dropped_log_count = buffer.dropped_log_count.saturating_add(1);
        }
        let Some(log) = buffer.logs.back().cloned() else {
            tracing::error!("job log buffer unexpectedly empty after append");
            return;
        };
        (
            buffer.next_sequence,
            buffer.dropped_log_count > 0,
            buffer.dropped_log_count,
            log,
        )
    };

    if let Ok(mut jobs) = jobs.lock() {
        if let Some(record) = jobs.get_mut(job_id) {
            record.log_count = log_count;
            record.logs_truncated = logs_truncated;
        }
    } else {
        tracing::error!("job map mutex poisoned");
    }
    match log_events.lock() {
        Ok(log_events) => {
            if let Some(sender) = log_events.get(job_id) {
                let _ = sender.send(log.clone());
            }
        }
        Err(_) => tracing::error!("job log event mutex poisoned"),
    }
    if let Err(error) = append_store_record(
        store_writer,
        &serde_json::json!({
            "kind": "job_log",
            "job_id": job_id,
            "log": log,
            "dropped_log_count": dropped_log_count,
        }),
    ) {
        tracing::warn!("failed to persist job log: {error:#}");
    }
}

pub(crate) fn finish_job(completion: &JobCompletion, status: JobStatus, exit_code: Option<i32>) {
    if let Ok(mut cancels) = completion.cancels.lock() {
        cancels.remove(&completion.job_id);
    } else {
        tracing::error!("job cancel mutex poisoned");
    }
    if let Ok(mut stdin) = completion.stdin.lock() {
        stdin.remove(&completion.job_id);
    } else {
        tracing::error!("job stdin mutex poisoned");
    }

    let terminal = {
        let Ok(mut jobs) = completion.jobs.lock() else {
            tracing::error!("job map mutex poisoned");
            cleanup_finished_job_runtime(completion);
            return;
        };
        if let Some(record) = jobs.get_mut(&completion.job_id) {
            record.status = status;
            record.exit_code = exit_code;
            let event = job_event_from_record(record);
            Some((event, record.clone()))
        } else {
            None
        }
    };
    if let Some((event, record)) = terminal {
        if let Err(error) = append_store_record(
            &completion.store_writer,
            &serde_json::json!({
                "kind": "job",
                "record": record,
            }),
        ) {
            tracing::warn!("failed to persist job record: {error:#}");
        }
        record_job_completion_audit(completion, &record);
        match completion.events.lock() {
            Ok(events) => {
                if let Some(sender) = events.get(&completion.job_id) {
                    let _ = sender.send(event);
                }
            }
            Err(_) => tracing::error!("job event mutex poisoned"),
        }
    }
    cleanup_finished_job_runtime(completion);
}

fn record_job_completion_audit(completion: &JobCompletion, record: &JobRecord) {
    let reason = match record.exit_code {
        Some(code) => format!(
            "status={} exit_code={code}",
            operon_protocol::format_job_status(&record.status)
        ),
        None => format!(
            "status={}",
            operon_protocol::format_job_status(&record.status)
        ),
    };
    push_audit_event(
        &completion.audit,
        &completion.store_writer,
        AuditEvent {
            subject: completion.subject.clone(),
            timestamp_ms: now_ms(),
            node_id: completion.node_id.clone(),
            capability: "job:default".to_string(),
            action: "finish".to_string(),
            resource: completion.job_id.clone(),
            allowed: true,
            reason,
            run_id: completion.audit_context.run_id.clone(),
            step_id: completion.audit_context.step_id.clone(),
        },
    );
}

fn cleanup_finished_job_runtime(completion: &JobCompletion) {
    if let Ok(mut events) = completion.events.lock() {
        events.remove(&completion.job_id);
    } else {
        tracing::error!("job event mutex poisoned");
    }
    if let Ok(mut log_events) = completion.log_events.lock() {
        log_events.remove(&completion.job_id);
    } else {
        tracing::error!("job log event mutex poisoned");
    }
    prune_completed_job_log_buffers(&completion.jobs, &completion.logs);
}

pub(crate) fn next_job_sequence(jobs: &BTreeMap<String, JobRecord>) -> u64 {
    jobs.keys()
        .filter_map(|id| job_sequence_number(id))
        .max()
        .unwrap_or(0)
        + 1
}

fn job_sequence_number(job_id: &str) -> Option<u64> {
    job_id.strip_prefix("job-")?.parse::<u64>().ok()
}

pub(crate) fn prune_completed_job_log_buffers(
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
) {
    let Ok(jobs) = jobs.lock() else {
        tracing::error!("job map mutex poisoned");
        return;
    };
    let Ok(logs_guard) = logs.lock() else {
        tracing::error!("job log mutex poisoned");
        return;
    };
    let mut completed_log_job_ids = logs_guard
        .keys()
        .filter(|job_id| {
            jobs.get(*job_id)
                .map(|record| !matches!(record.status, JobStatus::Running))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    drop(logs_guard);

    if completed_log_job_ids.len() <= MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS {
        return;
    }

    completed_log_job_ids.sort_by_key(|job_id| job_sequence_number(job_id).unwrap_or(u64::MAX));
    let remove_count = completed_log_job_ids.len() - MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS;
    drop(jobs);

    match logs.lock() {
        Ok(mut logs) => {
            for job_id in completed_log_job_ids.into_iter().take(remove_count) {
                logs.remove(&job_id);
            }
        }
        Err(_) => tracing::error!("job log mutex poisoned"),
    }
}

pub(crate) fn job_log_buffers_from_persisted_logs(
    persisted_logs: BTreeMap<String, Vec<JobLog>>,
) -> BTreeMap<String, JobLogBuffer> {
    let mut buffers = BTreeMap::new();
    for (job_id, logs) in persisted_logs {
        let mut buffer = JobLogBuffer::default();
        for log in logs {
            buffer.next_sequence = buffer.next_sequence.max(log.sequence.saturating_add(1));
            buffer.logs.push_back(log);
            while buffer.logs.len() > MAX_IN_MEMORY_JOB_LOGS {
                buffer.logs.pop_front();
                buffer.dropped_log_count = buffer.dropped_log_count.saturating_add(1);
            }
        }
        buffers.insert(job_id, buffer);
    }
    buffers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_job_logs_seed_bounded_log_buffers() {
        let logs = BTreeMap::from([(
            "job-1".to_string(),
            (0..MAX_IN_MEMORY_JOB_LOGS + 2)
                .map(|sequence| JobLog {
                    stream: "stdout".to_string(),
                    data: format!("line-{sequence}").into_bytes(),
                    sequence: sequence as u64,
                })
                .collect::<Vec<_>>(),
        )]);

        let buffers = job_log_buffers_from_persisted_logs(logs);
        let buffer = buffers.get("job-1").expect("job log buffer");

        assert_eq!(buffer.logs.len(), MAX_IN_MEMORY_JOB_LOGS);
        assert_eq!(buffer.logs.front().expect("first retained").sequence, 2);
        assert_eq!(buffer.next_sequence, (MAX_IN_MEMORY_JOB_LOGS + 2) as u64);
        assert_eq!(buffer.dropped_log_count, 2);
    }
}
