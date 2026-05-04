use std::{fs, io::Read, path::Path};

use anyhow::Context;
use futures_util::stream;
use operon_core::{
    ExecEvent, ExecList, ExecLogList, ExecRecord, ExecRunRequest, ExecStatus, ExecStdin,
    ExecStdinClose,
};
use operon_grpc_client::chunk_stdin_requests;
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    exec_log_stream_event, ExecCancelRequest, ExecIdRequest, ListExecsRequest,
};

use crate::grpc::{call, grpc_exec_run_request, with_auth, DEFAULT_LIST_PAGE_SIZE};

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
        let mut stream = client
            .watch_exec(with_auth(&endpoint, ExecIdRequest { exec_id })?)
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
        let mut stream = client
            .stream_exec_logs(with_auth(&endpoint, ExecIdRequest { exec_id })?)
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
        Ok(client
            .write_exec_stdin(with_auth(&endpoint, stream::iter(chunks))?)
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
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_exec_stdin_bytes(endpoint, exec_id, &data).await
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
