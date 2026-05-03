use std::pin::Pin;

use async_stream::stream;
use futures_util::StreamExt;
use operon_core::{ExecList, ExecRunRequest, ExecStatus, ExecStdin, ExecStdinClose};
use operon_protocol::runtime::v1::{
    exec_stdin_request, ExecIdRequest, ExecLogStreamEvent, ListExecsRequest,
};
use tokio::sync::broadcast;
use tonic::{Status, Streaming};

use crate::{
    exec_runtime::{
        exec_event_from_record, exec_log_complete, exec_log_complete_event, exec_log_entry_event,
        exec_log_list, exec_log_snapshot, exec_log_snapshot_event, get_exec_record, start_exec,
    },
    exec_session,
    locks::lock,
    pagination::paginate_items,
    state::{AppState, ExecEventSender, ExecLogSender},
};

pub(crate) type ExecLogStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<ExecLogStreamEvent, Status>> + Send + 'static>>;
pub(crate) type ExecEventStream = Pin<
    Box<
        dyn futures_util::Stream<Item = Result<operon_protocol::runtime::v1::ExecEvent, Status>>
            + Send
            + 'static,
    >,
>;
pub(crate) type ExecSessionStream = exec_session::ExecSessionStream;

pub(crate) fn run_exec(
    state: &AppState,
    request: operon_protocol::runtime::v1::ExecRunRequest,
) -> Result<operon_protocol::runtime::v1::ExecRecord, Status> {
    let record = start_exec(
        state,
        ExecRunRequest {
            command: request.command,
            argv: request.argv,
            cwd: (!request.cwd.is_empty()).then_some(request.cwd),
            timeout_secs: request.timeout_secs,
            secrets: request.secrets,
        },
    )?;
    Ok(record.into())
}

pub(crate) fn get_exec(
    state: &AppState,
    request: ExecIdRequest,
) -> Result<operon_protocol::runtime::v1::ExecRecord, Status> {
    Ok(get_exec_record(state, &request.exec_id)?.into())
}

pub(crate) fn list_execs(
    state: &AppState,
    request: ListExecsRequest,
) -> Result<operon_protocol::runtime::v1::ExecList, Status> {
    let execs = lock(&state.execs, "exec map")?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let (execs, next_page_token) = paginate_items(&execs, request.page_size, &request.page_token)?;
    let mut response: operon_protocol::runtime::v1::ExecList = ExecList {
        execs,
        next_page_token: String::new(),
    }
    .into();
    response.next_page_token = next_page_token;
    Ok(response)
}

pub(crate) fn watch_exec(state: AppState, exec_id: String) -> Result<ExecEventStream, Status> {
    let mut receiver = lock(&state.exec_events, "exec event")?
        .get(&exec_id)
        .map(ExecEventSender::subscribe);
    let initial = exec_event_from_record(&get_exec_record(&state, &exec_id)?);
    let stream = stream! {
        let mut latest = initial;
        yield Ok::<_, Status>(latest.clone().into());
        if !matches!(latest.status, ExecStatus::Running) {
            return;
        }
        if let Some(receiver) = receiver.as_mut() {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        latest = event;
                        yield Ok::<_, Status>(latest.clone().into());
                        if !matches!(latest.status, ExecStatus::Running) {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        match get_exec_record(&state, &exec_id) {
                            Ok(record) => {
                                latest = exec_event_from_record(&record);
                                yield Ok::<_, Status>(latest.clone().into());
                                if !matches!(latest.status, ExecStatus::Running) {
                                    break;
                                }
                            }
                            Err(status) => {
                                yield Err(status);
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };
    Ok(Box::pin(stream))
}

pub(crate) fn list_exec_logs(
    state: &AppState,
    exec_id: String,
) -> Result<operon_protocol::runtime::v1::ExecLogList, Status> {
    get_exec_record(state, &exec_id)?;
    Ok(exec_log_list(state, &exec_id)?.into())
}

pub(crate) fn stream_exec_logs(state: AppState, exec_id: String) -> Result<ExecLogStream, Status> {
    let mut log_receiver = lock(&state.exec_log_events, "exec log event")?
        .get(&exec_id)
        .map(ExecLogSender::subscribe);
    let mut event_receiver = lock(&state.exec_events, "exec event")?
        .get(&exec_id)
        .map(ExecEventSender::subscribe);
    let initial_record = get_exec_record(&state, &exec_id)?;
    let (initial_snapshot, mut next_sequence) = exec_log_snapshot(&state, &exec_id)?;
    let stream = stream! {
        yield Ok::<_, Status>(exec_log_snapshot_event(initial_snapshot));
        if !matches!(initial_record.status, ExecStatus::Running) {
            match exec_log_complete(&state, &exec_id) {
                Ok(complete) => yield Ok(exec_log_complete_event(complete)),
                Err(status) => yield Err(status),
            }
            return;
        }
        if let (Some(log_receiver), Some(event_receiver)) =
            (log_receiver.as_mut(), event_receiver.as_mut())
        {
            loop {
                tokio::select! {
                    log = log_receiver.recv() => {
                        match log {
                            Ok(log) => {
                                if log.sequence >= next_sequence {
                                    next_sequence = log.sequence.saturating_add(1);
                                    yield Ok::<_, Status>(exec_log_entry_event(&exec_id, log));
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                match exec_log_snapshot(&state, &exec_id) {
                                    Ok((snapshot, snapshot_next_sequence)) => {
                                        next_sequence = next_sequence.max(snapshot_next_sequence);
                                        yield Ok::<_, Status>(exec_log_snapshot_event(snapshot));
                                    }
                                    Err(status) => {
                                        yield Err(status);
                                        break;
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    event = event_receiver.recv() => {
                        match event {
                            Ok(event) => {
                                if !matches!(event.status, ExecStatus::Running) {
                                    match exec_log_snapshot(&state, &exec_id) {
                                        Ok((snapshot, _snapshot_next_sequence)) => {
                                            yield Ok::<_, Status>(exec_log_snapshot_event(snapshot));
                                        }
                                        Err(status) => {
                                            yield Err(status);
                                        }
                                    }
                                    match exec_log_complete(&state, &exec_id) {
                                        Ok(complete) => yield Ok::<_, Status>(exec_log_complete_event(complete)),
                                        Err(status) => yield Err(status),
                                    }
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        }
    };
    Ok(Box::pin(stream))
}

pub(crate) async fn write_exec_stdin(
    state: &AppState,
    stream: &mut Streaming<operon_protocol::runtime::v1::ExecStdinRequest>,
) -> Result<operon_protocol::runtime::v1::ExecStdin, Status> {
    let mut exec_id = None;
    let mut sender = None;
    let mut bytes_written = 0_u64;
    while let Some(message) = stream.next().await {
        let message = message?;
        match message.payload {
            Some(exec_stdin_request::Payload::Target(target)) => {
                if exec_id.is_some() {
                    return Err(Status::invalid_argument(
                        "stdin stream target metadata was sent more than once",
                    ));
                }
                if target.exec_id.is_empty() {
                    return Err(Status::invalid_argument(
                        "stdin stream target exec_id is required",
                    ));
                }
                sender = Some(
                    lock(&state.exec_stdin, "exec stdin")?
                        .get(&target.exec_id)
                        .cloned()
                        .ok_or_else(|| {
                            Status::not_found(format!(
                                "exec `{}` has no open stdin",
                                target.exec_id
                            ))
                        })?,
                );
                exec_id = Some(target.exec_id);
            }
            Some(exec_stdin_request::Payload::Chunk(chunk)) => {
                let Some(sender) = &sender else {
                    return Err(Status::invalid_argument(
                        "stdin stream chunk arrived before target metadata",
                    ));
                };
                bytes_written += chunk.data.len() as u64;
                sender
                    .send(chunk.data)
                    .map_err(|_| Status::failed_precondition("exec stdin is closed"))?;
            }
            None => {
                return Err(Status::invalid_argument(
                    "stdin stream message is missing payload",
                ));
            }
        }
    }
    let Some(exec_id) = exec_id else {
        return Err(Status::invalid_argument(
            "stdin stream did not include target metadata",
        ));
    };
    Ok(ExecStdin {
        exec_id,
        bytes_written,
    }
    .into())
}

pub(crate) fn close_exec_stdin(
    state: &AppState,
    exec_id: String,
) -> Result<operon_protocol::runtime::v1::ExecStdinClose, Status> {
    let closed = lock(&state.exec_stdin, "exec stdin")?
        .remove(&exec_id)
        .is_some();
    Ok(ExecStdinClose { exec_id, closed }.into())
}

pub(crate) fn cancel_exec(
    state: &AppState,
    exec_id: String,
) -> Result<operon_protocol::runtime::v1::ExecRecord, Status> {
    if let Some(sender) = lock(&state.exec_cancel, "exec cancel")?.remove(&exec_id) {
        let _ = sender.send(());
        crate::audit::record_audit_capability(
            state,
            "exec:default",
            "cancel",
            &exec_id,
            true,
            "cancel requested",
        );
    }
    Ok(get_exec_record(state, &exec_id)?.into())
}

pub(crate) async fn open_exec_session(
    state: AppState,
    stream: Streaming<operon_protocol::runtime::v1::ExecSessionRequest>,
) -> Result<ExecSessionStream, Status> {
    exec_session::open_exec_session(state, stream).await
}
