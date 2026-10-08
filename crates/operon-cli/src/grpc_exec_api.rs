use std::path::Path;

use anyhow::Context;
use futures_util::{stream, StreamExt};
use operon_core::runtime::NodeEndpoint;
use operon_core::{
    ExecEvent, ExecList, ExecLogList, ExecRecord, ExecRunRequest, ExecStatus, ExecStdin,
    ExecStdinClose,
};
use operon_grpc_client::chunk_stdin_requests;
use operon_protocol::runtime::v1::{
    exec_log_stream_event, ExecCancelRequest, ExecIdRequest, ListExecsRequest,
};
use tokio::io::AsyncReadExt;

use crate::grpc::{
    call, grpc_exec_run_request, stream_response, with_auth, with_auth_stream,
    DEFAULT_LIST_PAGE_SIZE,
};

pub async fn run_exec(
    endpoint: &NodeEndpoint,
    request: ExecRunRequest,
) -> anyhow::Result<ExecRecord> {
    call(endpoint, |mut client, endpoint| async move {
        client
            .run_exec(with_auth(&endpoint, grpc_exec_run_request(request))?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub async fn get_exec(endpoint: &NodeEndpoint, exec_id: &str) -> anyhow::Result<ExecRecord> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        client
            .get_exec(with_auth(&endpoint, ExecIdRequest { exec_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub async fn list_execs(endpoint: &NodeEndpoint) -> anyhow::Result<ExecList> {
    let mut execs = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_execs(with_auth(
                        &endpoint,
                        ListExecsRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        execs.extend(
            response
                .execs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(anyhow::Error::msg)?,
        );
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(ExecList {
        execs,
        next_page_token: String::new(),
    })
}

pub async fn watch_exec_to_terminal(
    endpoint: &NodeEndpoint,
    exec_id: &str,
) -> anyhow::Result<ExecEvent> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = stream_response(
            &endpoint,
            client.watch_exec(with_auth_stream(&endpoint, ExecIdRequest { exec_id })?),
        )
        .await?
        .into_inner();
        let mut latest = None;
        while let Some(event) = stream.message().await? {
            let event: ExecEvent = event.try_into().map_err(anyhow::Error::msg)?;
            let terminal = !matches!(event.status, ExecStatus::Running);
            latest = Some(event);
            if terminal {
                break;
            }
        }
        latest.ok_or_else(|| anyhow::anyhow!("exec watch stream ended without an event"))
    })
    .await
}

pub async fn list_exec_logs(endpoint: &NodeEndpoint, exec_id: &str) -> anyhow::Result<ExecLogList> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .list_exec_logs(with_auth(&endpoint, ExecIdRequest { exec_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn stream_exec_logs(
    endpoint: &NodeEndpoint,
    exec_id: &str,
) -> anyhow::Result<ExecLogList> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let response_exec_id = exec_id.clone();
        let mut stream = stream_response(
            &endpoint,
            client.stream_exec_logs(with_auth_stream(&endpoint, ExecIdRequest { exec_id })?),
        )
        .await?
        .into_inner();
        let mut logs = Vec::new();
        let mut truncated = false;
        let mut dropped_log_count = 0;
        let mut next_sequence = 0;
        while let Some(event) = stream.message().await? {
            match event.event {
                Some(exec_log_stream_event::Event::Snapshot(snapshot)) => {
                    truncated = snapshot.truncated;
                    dropped_log_count = snapshot.dropped_log_count;
                    for log in snapshot.logs {
                        if log.sequence >= next_sequence {
                            next_sequence = log.sequence.saturating_add(1);
                            logs.push(log.into());
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
                        logs.push(log.into());
                    }
                }
                Some(exec_log_stream_event::Event::Complete(complete)) => {
                    truncated = complete.truncated;
                    dropped_log_count = complete.dropped_log_count;
                }
                None => {}
            }
        }
        Ok(ExecLogList {
            exec_id: response_exec_id,
            logs,
            truncated,
            dropped_log_count,
        })
    })
    .await
}

pub async fn write_exec_stdin_bytes(
    endpoint: &NodeEndpoint,
    exec_id: &str,
    body: &[u8],
) -> anyhow::Result<ExecStdin> {
    let chunks = chunk_stdin_requests(exec_id.to_string(), body);
    call(endpoint, |mut client, endpoint| async move {
        let (progress, receiver) = tokio::sync::watch::channel(0_u64);
        let outbound = stream::iter(chunks).inspect(move |_| {
            progress.send_modify(|count| *count += 1);
        });
        Ok(operon_grpc_client::transfer_progress(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs),
            receiver,
            client.write_exec_stdin(operon_grpc_client::with_deadline(
                with_auth_stream(&endpoint, outbound)?,
                operon_core::runtime::TransportConfig::timeout(
                    endpoint.transport.transfer_timeout_secs,
                ),
            )),
        )
        .await?
        .into_inner()
        .into())
    })
    .await
}

pub async fn write_exec_stdin_file(
    endpoint: &NodeEndpoint,
    exec_id: &str,
    file: &Path,
) -> anyhow::Result<ExecStdin> {
    let file = tokio::fs::File::open(file)
        .await
        .with_context(|| format!("failed to open {}", file.display()))?;
    let target = chunk_stdin_requests(exec_id.to_string(), &[])
        .next()
        .ok_or_else(|| anyhow::anyhow!("stdin target metadata unavailable"))?;
    call(endpoint, |mut client, endpoint| async move {
        let mut source = file;
        let (read_error, read_failure) = tokio::sync::oneshot::channel();
        let (progress, receiver) = tokio::sync::watch::channel(0_u64);
        let outbound = async_stream::stream! {
            let mut read_error = Some(read_error);
            let mut sent_data = false;
            progress.send_modify(|count| *count += 1);
            yield target;
            loop {
                let mut data = Vec::with_capacity(operon_grpc_client::STREAM_CHUNK_BYTES);
                match source.read_buf(&mut data).await {
                    Ok(0) => break,
                    Ok(_) => {
                        sent_data = true;
                        progress.send_modify(|count| *count += 1);
                        yield operon_protocol::runtime::v1::ExecStdinRequest {
                            payload: Some(operon_protocol::runtime::v1::exec_stdin_request::Payload::Chunk(
                                operon_protocol::runtime::v1::FileChunk { data }
                            )),
                        };
                    }
                    Err(error) => {
                        if let Some(sender) = read_error.take() { let _ = sender.send(error); }
                        std::future::pending::<()>().await;
                        return;
                    }
                }
            }
            if !sent_data {
                progress.send_modify(|count| *count += 1);
                yield operon_protocol::runtime::v1::ExecStdinRequest {
                    payload: Some(operon_protocol::runtime::v1::exec_stdin_request::Payload::Chunk(
                        operon_protocol::runtime::v1::FileChunk { data: Vec::new() }
                    )),
                };
            }
        };
        let rpc = operon_grpc_client::transfer_progress(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs), receiver,
            client.write_exec_stdin(operon_grpc_client::with_deadline(
                with_auth_stream(&endpoint, outbound)?,
                operon_core::runtime::TransportConfig::timeout(endpoint.transport.transfer_timeout_secs),
            )),
        );
        let local_error = async {
            match read_failure.await {
                Ok(error) => error,
                Err(_) => std::future::pending::<std::io::Error>().await,
            }
        };
        tokio::select! {
            biased;
            error = local_error => Err(anyhow::Error::new(error).context("failed to read stdin source; already sent bytes cannot be rolled back")),
            result = rpc => Ok(result?.into_inner().into()),
        }
    }).await
}

pub async fn close_exec_stdin(
    endpoint: &NodeEndpoint,
    exec_id: &str,
) -> anyhow::Result<ExecStdinClose> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .close_exec_stdin(with_auth(&endpoint, ExecIdRequest { exec_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn cancel_exec(endpoint: &NodeEndpoint, exec_id: &str) -> anyhow::Result<ExecRecord> {
    let exec_id = exec_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        client
            .cancel_exec(with_auth(&endpoint, ExecCancelRequest { exec_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}
