use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

use anyhow::Context;
use futures_util::stream;
use operon_core::{
    AuditLog, CapabilityList, FsList, FsStat, FsWrite, HealthStatus, JobEvent, JobList, JobLogList,
    JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose, NodeInfo, RequestContext,
    ServiceCheck, ServiceList,
};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    job_log_stream_event, job_stdin_request, operon_runtime_client::OperonRuntimeClient,
    write_file_request, FileChunk, FsCopyRequest, FsPathRequest, FsRenameRequest,
    FsTruncateRequest, GetNodeRequest, HealthRequest, JobCancelRequest, JobIdRequest,
    JobStdinRequest, JobStdinTarget, ListAuditRequest, ListCapabilitiesRequest, ListJobsRequest,
    ListServicesRequest, ServiceIdRequest, WriteFileRequest, WriteFileTarget,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};

const DEFAULT_LIST_PAGE_SIZE: u32 = 1000;

tokio::task_local! {
    static REQUEST_CONTEXT: RequestContext;
}

pub async fn with_request_context<T, Fut>(
    context: RequestContext,
    f: impl FnOnce() -> Fut,
) -> anyhow::Result<T>
where
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    REQUEST_CONTEXT.scope(context, f()).await
}

pub async fn health_and_node(endpoint: &NodeEndpoint) -> anyhow::Result<(HealthStatus, NodeInfo)> {
    call(endpoint, |mut client, endpoint| async move {
        let health = client
            .health(with_auth(&endpoint, HealthRequest {})?)
            .await?
            .into_inner()
            .into();
        let node = client
            .get_node(with_auth(&endpoint, GetNodeRequest {})?)
            .await?
            .into_inner()
            .into();
        Ok((health, node))
    })
    .await
}

pub async fn list_capabilities(endpoint: &NodeEndpoint) -> anyhow::Result<CapabilityList> {
    let mut capabilities = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_capabilities(with_auth(
                        &endpoint,
                        ListCapabilitiesRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        capabilities.extend(
            response
                .capabilities
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
    Ok(CapabilityList {
        capabilities,
        next_page_token: String::new(),
    })
}

pub async fn fs_stat(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .stat_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn fs_list(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsList> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .list_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn read_file_to_writer(
    endpoint: &NodeEndpoint,
    path: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .read_file(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner();
        while let Some(chunk) = stream.message().await? {
            writer.write_all(&chunk.data)?;
        }
        Ok(())
    })
    .await
}

pub async fn write_file_bytes(
    endpoint: &NodeEndpoint,
    path: &str,
    body: &[u8],
) -> anyhow::Result<FsWrite> {
    let path = path.to_string();
    let chunks = chunk_write_requests(path, body);
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .write_file(with_auth(&endpoint, stream::iter(chunks))?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn write_file(
    endpoint: &NodeEndpoint,
    path: &str,
    file: &Path,
) -> anyhow::Result<FsWrite> {
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_file_bytes(endpoint, path, &data).await
}

pub async fn fs_mkdir(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .mkdir_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn fs_delete(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<String> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .delete_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .path)
    })
    .await
}

pub async fn fs_rename(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String)> {
    let request = FsRenameRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
    };
    call(endpoint, |mut client, endpoint| async move {
        let response = client
            .rename_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path))
    })
    .await
}

pub async fn fs_copy(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String, u64)> {
    let request = FsCopyRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
    };
    call(endpoint, |mut client, endpoint| async move {
        let response = client
            .copy_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path, response.bytes_copied))
    })
    .await
}

pub async fn fs_truncate(endpoint: &NodeEndpoint, path: &str, size: u64) -> anyhow::Result<FsStat> {
    let request = FsTruncateRequest {
        path: path.to_string(),
        size,
    };
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .truncate_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn run_job(endpoint: &NodeEndpoint, request: JobRunRequest) -> anyhow::Result<JobRecord> {
    call(endpoint, |mut client, endpoint| async move {
        client
            .run_job(with_auth(&endpoint, grpc_job_run_request(request))?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub async fn get_job(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobRecord> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        client
            .get_job(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub async fn list_jobs(endpoint: &NodeEndpoint) -> anyhow::Result<JobList> {
    let mut jobs = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_jobs(with_auth(
                        &endpoint,
                        ListJobsRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        jobs.extend(
            response
                .jobs
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
    Ok(JobList {
        jobs,
        next_page_token: String::new(),
    })
}

pub async fn watch_job_to_terminal(
    endpoint: &NodeEndpoint,
    job_id: &str,
) -> anyhow::Result<JobEvent> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .watch_job(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner();
        let mut latest = None;
        while let Some(event) = stream.message().await? {
            let event: JobEvent = event.try_into().map_err(anyhow::Error::msg)?;
            let terminal = !matches!(event.status, JobStatus::Running);
            latest = Some(event);
            if terminal {
                break;
            }
        }
        latest.ok_or_else(|| anyhow::anyhow!("job watch stream ended without an event"))
    })
    .await
}

pub async fn list_job_logs(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobLogList> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .list_job_logs(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn stream_job_logs_to_writer(
    endpoint: &NodeEndpoint,
    job_id: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .stream_job_logs(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner();
        let mut next_sequence = 0;
        while let Some(event) = stream.message().await? {
            match event.event {
                Some(job_log_stream_event::Event::Snapshot(snapshot)) => {
                    for log in snapshot.logs {
                        if log.sequence >= next_sequence {
                            next_sequence = log.sequence.saturating_add(1);
                            writer.write_all(&log.data)?;
                        }
                    }
                    next_sequence = next_sequence.max(snapshot.next_sequence);
                }
                Some(job_log_stream_event::Event::Entry(entry)) => {
                    let Some(log) = entry.log else {
                        continue;
                    };
                    if log.sequence >= next_sequence {
                        next_sequence = log.sequence.saturating_add(1);
                        writer.write_all(&log.data)?;
                    }
                }
                Some(job_log_stream_event::Event::Complete(_)) | None => {}
            }
        }
        Ok(())
    })
    .await
}

pub async fn stream_job_logs(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobLogList> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let response_job_id = job_id.clone();
        let mut stream = client
            .stream_job_logs(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner();
        let mut logs = Vec::new();
        let mut truncated = false;
        let mut dropped_log_count = 0;
        let mut next_sequence = 0;
        while let Some(event) = stream.message().await? {
            match event.event {
                Some(job_log_stream_event::Event::Snapshot(snapshot)) => {
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
                Some(job_log_stream_event::Event::Entry(entry)) => {
                    let Some(log) = entry.log else {
                        continue;
                    };
                    if log.sequence >= next_sequence {
                        next_sequence = log.sequence.saturating_add(1);
                        logs.push(log.into());
                    }
                }
                Some(job_log_stream_event::Event::Complete(complete)) => {
                    truncated = complete.truncated;
                    dropped_log_count = complete.dropped_log_count;
                }
                None => {}
            }
        }
        Ok(JobLogList {
            job_id: response_job_id,
            logs,
            truncated,
            dropped_log_count,
        })
    })
    .await
}

pub async fn write_job_stdin_bytes(
    endpoint: &NodeEndpoint,
    job_id: &str,
    body: &[u8],
) -> anyhow::Result<JobStdin> {
    let chunks = chunk_stdin_requests(job_id.to_string(), body);
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .write_job_stdin(with_auth(&endpoint, stream::iter(chunks))?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn write_job_stdin_file(
    endpoint: &NodeEndpoint,
    job_id: &str,
    file: &Path,
) -> anyhow::Result<JobStdin> {
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_job_stdin_bytes(endpoint, job_id, &data).await
}

pub async fn close_job_stdin(
    endpoint: &NodeEndpoint,
    job_id: &str,
) -> anyhow::Result<JobStdinClose> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .close_job_stdin(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn cancel_job(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobRecord> {
    let job_id = job_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        client
            .cancel_job(with_auth(&endpoint, JobCancelRequest { job_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub async fn list_services(endpoint: &NodeEndpoint) -> anyhow::Result<ServiceList> {
    let mut services = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_services(with_auth(
                        &endpoint,
                        ListServicesRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        services.extend(
            response
                .services
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
    Ok(ServiceList {
        services,
        next_page_token: String::new(),
    })
}

pub async fn check_service(
    endpoint: &NodeEndpoint,
    service_id: &str,
) -> anyhow::Result<ServiceCheck> {
    let service_id = service_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .check_service(with_auth(&endpoint, ServiceIdRequest { service_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn list_audit(endpoint: &NodeEndpoint) -> anyhow::Result<AuditLog> {
    let mut events = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_audit(with_auth(
                        &endpoint,
                        ListAuditRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        events.extend(response.events.into_iter().map(Into::into));
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(AuditLog {
        events,
        next_page_token: String::new(),
    })
}

async fn call<T, Fut>(
    endpoint: &NodeEndpoint,
    f: impl FnOnce(OperonRuntimeClient<Channel>, NodeEndpoint) -> Fut,
) -> anyhow::Result<T>
where
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let endpoint = endpoint.clone();
    let channel = Channel::from_shared(grpc_channel_uri(&endpoint.endpoint)?)?
        .connect()
        .await?;
    f(OperonRuntimeClient::new(channel), endpoint).await
}

fn with_auth<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<Request<T>> {
    let mut request = Request::new(message);
    if let Some(token) = &endpoint.token {
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))?,
        );
    }
    if let Ok(context) = REQUEST_CONTEXT.try_with(Clone::clone) {
        if let Some(run_id) = &context.run_id {
            request
                .metadata_mut()
                .insert("x-operon-run-id", MetadataValue::try_from(run_id.as_str())?);
        }
        if let Some(step_id) = &context.step_id {
            request.metadata_mut().insert(
                "x-operon-step-id",
                MetadataValue::try_from(step_id.as_str())?,
            );
        }
    }
    Ok(request)
}

fn grpc_channel_uri(endpoint: &str) -> anyhow::Result<String> {
    if let Some(rest) = endpoint.strip_prefix("grpc://") {
        Ok(format!("http://{rest}"))
    } else if let Some(rest) = endpoint.strip_prefix("grpcs://") {
        Ok(format!("https://{rest}"))
    } else {
        anyhow::bail!("only grpc:// and grpcs:// endpoints are supported by the gRPC client")
    }
}

fn chunk_write_requests(path: String, body: &[u8]) -> Vec<WriteFileRequest> {
    let mut chunks = vec![WriteFileRequest {
        payload: Some(write_file_request::Payload::Target(WriteFileTarget {
            path,
        })),
    }];
    if body.is_empty() {
        chunks.push(WriteFileRequest {
            payload: Some(write_file_request::Payload::Chunk(FileChunk {
                data: Vec::new(),
            })),
        });
        return chunks;
    }
    chunks.extend(body.chunks(64 * 1024).map(|chunk| WriteFileRequest {
        payload: Some(write_file_request::Payload::Chunk(FileChunk {
            data: chunk.to_vec(),
        })),
    }));
    chunks
}

fn chunk_stdin_requests(job_id: String, body: &[u8]) -> Vec<JobStdinRequest> {
    let mut chunks = vec![JobStdinRequest {
        payload: Some(job_stdin_request::Payload::Target(JobStdinTarget {
            job_id,
        })),
    }];
    if body.is_empty() {
        chunks.push(JobStdinRequest {
            payload: Some(job_stdin_request::Payload::Chunk(FileChunk {
                data: Vec::new(),
            })),
        });
        return chunks;
    }
    chunks.extend(body.chunks(64 * 1024).map(|chunk| JobStdinRequest {
        payload: Some(job_stdin_request::Payload::Chunk(FileChunk {
            data: chunk.to_vec(),
        })),
    }));
    chunks
}

fn grpc_job_run_request(value: JobRunRequest) -> operon_protocol::runtime::v1::JobRunRequest {
    operon_protocol::runtime::v1::JobRunRequest {
        command: value.command,
        cwd: value.cwd.unwrap_or_default(),
        timeout_secs: value.timeout_secs,
        secrets: value.secrets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_grpc_uri_to_tonic_http_uri() {
        assert_eq!(
            grpc_channel_uri("grpc://127.0.0.1:7789").expect("uri"),
            "http://127.0.0.1:7789"
        );
    }

    #[test]
    fn chunks_write_requests_use_target_then_data_chunks() {
        let chunks = chunk_write_requests("file.txt".to_string(), &[1_u8; 70 * 1024]);

        assert_eq!(chunks.len(), 3);
        assert!(matches!(
            chunks[0].payload.as_ref(),
            Some(write_file_request::Payload::Target(target)) if target.path == "file.txt"
        ));
        assert!(matches!(
            chunks[1].payload.as_ref(),
            Some(write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 64 * 1024
        ));
        assert!(matches!(
            chunks[2].payload.as_ref(),
            Some(write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 6 * 1024
        ));
    }

    #[tokio::test]
    async fn with_auth_includes_execution_context_metadata() {
        let endpoint = NodeEndpoint {
            node_id: "node-a".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            provider: operon_network::NetworkProviderKind::Manual,
            token: Some("test-token".to_string()),
        };

        with_request_context(
            RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
            || async {
                let request = with_auth(&endpoint, HealthRequest {})?;
                assert_eq!(
                    request
                        .metadata()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer test-token")
                );
                assert_eq!(
                    request
                        .metadata()
                        .get("x-operon-run-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("run-1")
                );
                assert_eq!(
                    request
                        .metadata()
                        .get("x-operon-step-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("step-1")
                );
                Ok(())
            },
        )
        .await
        .expect("context metadata");
    }
}
