use std::{
    io::{Read as _, Write},
    thread,
};

use operon_core::{ExecSessionEvent, ExecSessionStart, ExecStatus};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    exec_log_stream_event, exec_session_event, exec_session_request, ExecIdRequest,
    ExecSessionInput, ExecSessionRequest, ExecSessionResize,
};
use tokio::sync::mpsc;

use crate::grpc::{call, with_auth};

pub(crate) enum ExecSessionInputSource {
    Inline(Vec<u8>),
    LocalStdin { forward_resize: bool },
}

pub(crate) async fn stream_exec_logs_to_writer(
    endpoint: &NodeEndpoint,
    exec_id: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .stream_exec_logs(with_auth(&endpoint, ExecIdRequest { exec_id })?)
            .await?
            .into_inner();
        let mut next_sequence = 0;
        while let Some(event) = stream.message().await? {
            match event.event {
                Some(exec_log_stream_event::Event::Snapshot(snapshot)) => {
                    for log in snapshot.logs {
                        if log.sequence >= next_sequence {
                            next_sequence = log.sequence.saturating_add(1);
                            writer.write_all(&log.data)?;
                        }
                    }
                    next_sequence = next_sequence.max(snapshot.next_sequence);
                }
                Some(exec_log_stream_event::Event::Entry(entry)) => {
                    let Some(log) = entry.log else {
                        continue;
                    };
                    if log.sequence >= next_sequence {
                        next_sequence = log.sequence.saturating_add(1);
                        writer.write_all(&log.data)?;
                    }
                }
                Some(exec_log_stream_event::Event::Complete(_)) | None => {}
            }
        }
        Ok(())
    })
    .await
}

pub(crate) async fn open_exec_session_to_writer(
    endpoint: &NodeEndpoint,
    start: ExecSessionStart,
    input: ExecSessionInputSource,
    writer: &mut impl Write,
) -> anyhow::Result<ExecSessionEvent> {
    call(endpoint, |mut client, endpoint| async move {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(ExecSessionRequest {
            payload: Some(exec_session_request::Payload::Start(start.into())),
        })?;
        match input {
            ExecSessionInputSource::Inline(input) => {
                tx.send(ExecSessionRequest {
                    payload: Some(exec_session_request::Payload::Input(ExecSessionInput {
                        data: input,
                    })),
                })?;
                drop(tx);
            }
            ExecSessionInputSource::LocalStdin { forward_resize } => {
                if forward_resize {
                    spawn_resize_forwarder(tx.clone());
                }
                spawn_stdin_forwarder(tx);
            }
        }
        let request_stream = async_stream::stream! {
            while let Some(message) = rx.recv().await {
                yield message;
            }
        };
        let mut stream = client
            .open_exec_session(with_auth(&endpoint, request_stream)?)
            .await?
            .into_inner();
        let mut terminal = None;
        while let Some(event) = stream.message().await? {
            match event.event {
                Some(exec_session_event::Event::Started(started)) => {
                    terminal = Some(ExecSessionEvent::Started(operon_core::ExecSessionStarted {
                        exec_id: started.exec_id,
                    }));
                }
                Some(exec_session_event::Event::Output(output)) => {
                    writer.write_all(&output.data)?;
                    terminal = Some(ExecSessionEvent::Output(operon_core::ExecSessionOutput {
                        exec_id: output.exec_id,
                        data: output.data,
                    }));
                }
                Some(exec_session_event::Event::Exit(exit)) => {
                    let status = operon_protocol::runtime::v1::ExecStatus::try_from(exit.status)
                        .ok()
                        .and_then(|status| match status {
                            operon_protocol::runtime::v1::ExecStatus::Running => {
                                Some(ExecStatus::Running)
                            }
                            operon_protocol::runtime::v1::ExecStatus::Succeeded => {
                                Some(ExecStatus::Succeeded)
                            }
                            operon_protocol::runtime::v1::ExecStatus::Failed => {
                                Some(ExecStatus::Failed)
                            }
                            operon_protocol::runtime::v1::ExecStatus::Cancelled => {
                                Some(ExecStatus::Cancelled)
                            }
                            operon_protocol::runtime::v1::ExecStatus::TimedOut => {
                                Some(ExecStatus::TimedOut)
                            }
                            operon_protocol::runtime::v1::ExecStatus::Unspecified => None,
                        })
                        .ok_or_else(|| {
                            anyhow::anyhow!("exec session exit status is unspecified")
                        })?;
                    terminal = Some(ExecSessionEvent::Exit(operon_core::ExecSessionExit {
                        exec_id: exit.exec_id,
                        status,
                        exit_code: exit.exit_code,
                    }));
                    break;
                }
                None => {}
            }
        }
        terminal.ok_or_else(|| anyhow::anyhow!("exec session ended without terminal event"))
    })
    .await
}

#[cfg(unix)]
fn spawn_resize_forwarder(tx: mpsc::UnboundedSender<ExecSessionRequest>) {
    tokio::spawn(async move {
        let Ok(mut signals) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
        else {
            return;
        };
        while signals.recv().await.is_some() {
            let Ok((cols, rows)) = crossterm::terminal::size() else {
                continue;
            };
            if tx
                .send(ExecSessionRequest {
                    payload: Some(exec_session_request::Payload::Resize(ExecSessionResize {
                        rows: u32::from(rows),
                        cols: u32::from(cols),
                    })),
                })
                .is_err()
            {
                break;
            }
        }
    });
}

#[cfg(not(unix))]
fn spawn_resize_forwarder(_tx: mpsc::UnboundedSender<ExecSessionRequest>) {}

fn spawn_stdin_forwarder(tx: mpsc::UnboundedSender<ExecSessionRequest>) {
    thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            if tx
                .send(ExecSessionRequest {
                    payload: Some(exec_session_request::Payload::Input(ExecSessionInput {
                        data: buffer[..read].to_vec(),
                    })),
                })
                .is_err()
            {
                break;
            }
        }
    });
}
