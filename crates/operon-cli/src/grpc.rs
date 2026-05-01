use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::OnceLock,
};

use anyhow::Context;
use futures_util::stream;
use operon_core::{
    AuditLog, CapabilityList, FsList, FsStat, FsWrite, HealthStatus, JobList, JobRecord,
    JobRunRequest, JobStdin, JobStdinClose, NodeInfo, ServiceCheck, ServiceList,
};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    operon_runtime_client::OperonRuntimeClient, FsCopyRequest, FsPathRequest, FsRenameRequest,
    FsTruncateRequest, GetNodeRequest, HealthRequest, JobCancelRequest, JobIdRequest,
    JobStdinRequest, ListAuditRequest, ListCapabilitiesRequest, ListJobsRequest,
    ListServicesRequest, ServiceIdRequest, WriteFileRequest,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};

static CLI_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn health_and_node(endpoint: &NodeEndpoint) -> anyhow::Result<(HealthStatus, NodeInfo)> {
    block_on(endpoint, |mut client, endpoint| async move {
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
}

pub fn list_capabilities(endpoint: &NodeEndpoint) -> anyhow::Result<CapabilityList> {
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .list_capabilities(with_auth(&endpoint, ListCapabilitiesRequest {})?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn fs_stat(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .stat_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn fs_list(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsList> {
    let path = path.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .list_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn read_file_to_writer(
    endpoint: &NodeEndpoint,
    path: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let path = path.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .read_file(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner();
        let mut data = Vec::new();
        while let Some(chunk) = stream.message().await? {
            data.extend_from_slice(&chunk.data);
        }
        Ok(data)
    })
    .and_then(|data| {
        writer.write_all(&data)?;
        Ok(())
    })
}

pub fn write_file_bytes(
    endpoint: &NodeEndpoint,
    path: &str,
    body: &[u8],
) -> anyhow::Result<FsWrite> {
    let path = path.to_string();
    let chunks = chunk_write_requests(path, body);
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .write_file(with_auth(&endpoint, stream::iter(chunks))?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn write_file(endpoint: &NodeEndpoint, path: &str, file: &Path) -> anyhow::Result<FsWrite> {
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_file_bytes(endpoint, path, &data)
}

pub fn fs_mkdir(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .mkdir_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn fs_delete(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<String> {
    let path = path.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .delete_fs(with_auth(&endpoint, FsPathRequest { path })?)
            .await?
            .into_inner()
            .path)
    })
}

pub fn fs_rename(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String)> {
    let request = FsRenameRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
    };
    block_on(endpoint, |mut client, endpoint| async move {
        let response = client
            .rename_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path))
    })
}

pub fn fs_copy(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String, u64)> {
    let request = FsCopyRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
    };
    block_on(endpoint, |mut client, endpoint| async move {
        let response = client
            .copy_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path, response.bytes_copied))
    })
}

pub fn fs_truncate(endpoint: &NodeEndpoint, path: &str, size: u64) -> anyhow::Result<FsStat> {
    let request = FsTruncateRequest {
        path: path.to_string(),
        size,
    };
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .truncate_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn run_job(endpoint: &NodeEndpoint, request: JobRunRequest) -> anyhow::Result<JobRecord> {
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .run_job(with_auth(&endpoint, grpc_job_run_request(request))?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn get_job(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobRecord> {
    let job_id = job_id.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .get_job(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn list_jobs(endpoint: &NodeEndpoint) -> anyhow::Result<JobList> {
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .list_jobs(with_auth(&endpoint, ListJobsRequest {})?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn stream_job_logs_to_writer(
    endpoint: &NodeEndpoint,
    job_id: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let job_id = job_id.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        let mut stream = client
            .stream_job_logs(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner();
        let mut data = Vec::new();
        while let Some(log) = stream.message().await? {
            data.extend_from_slice(log.data.as_bytes());
        }
        Ok(data)
    })
    .and_then(|data| {
        writer.write_all(&data)?;
        Ok(())
    })
}

pub fn write_job_stdin_bytes(
    endpoint: &NodeEndpoint,
    job_id: &str,
    body: &[u8],
) -> anyhow::Result<JobStdin> {
    let chunks = chunk_stdin_requests(job_id.to_string(), body);
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .write_job_stdin(with_auth(&endpoint, stream::iter(chunks))?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn write_job_stdin_file(
    endpoint: &NodeEndpoint,
    job_id: &str,
    file: &Path,
) -> anyhow::Result<JobStdin> {
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_job_stdin_bytes(endpoint, job_id, &data)
}

pub fn close_job_stdin(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobStdinClose> {
    let job_id = job_id.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .close_job_stdin(with_auth(&endpoint, JobIdRequest { job_id })?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn cancel_job(endpoint: &NodeEndpoint, job_id: &str) -> anyhow::Result<JobRecord> {
    let job_id = job_id.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .cancel_job(with_auth(&endpoint, JobCancelRequest { job_id })?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn list_services(endpoint: &NodeEndpoint) -> anyhow::Result<ServiceList> {
    block_on(endpoint, |mut client, endpoint| async move {
        client
            .list_services(with_auth(&endpoint, ListServicesRequest {})?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
}

pub fn check_service(endpoint: &NodeEndpoint, service_id: &str) -> anyhow::Result<ServiceCheck> {
    let service_id = service_id.to_string();
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .check_service(with_auth(&endpoint, ServiceIdRequest { service_id })?)
            .await?
            .into_inner()
            .into())
    })
}

pub fn list_audit(endpoint: &NodeEndpoint) -> anyhow::Result<AuditLog> {
    block_on(endpoint, |mut client, endpoint| async move {
        Ok(client
            .list_audit(with_auth(&endpoint, ListAuditRequest {})?)
            .await?
            .into_inner()
            .into())
    })
}

fn block_on<T, Fut>(
    endpoint: &NodeEndpoint,
    f: impl FnOnce(OperonRuntimeClient<Channel>, NodeEndpoint) -> Fut,
) -> anyhow::Result<T>
where
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let endpoint = endpoint.clone();
    let runtime = cli_runtime()?;
    runtime.block_on(async {
        let channel = Channel::from_shared(grpc_channel_uri(&endpoint.endpoint)?)?
            .connect()
            .await?;
        f(OperonRuntimeClient::new(channel), endpoint).await
    })
}

fn cli_runtime() -> anyhow::Result<&'static tokio::runtime::Runtime> {
    if let Some(runtime) = CLI_RUNTIME.get() {
        return Ok(runtime);
    }

    let runtime = tokio::runtime::Runtime::new().context("create CLI tokio runtime")?;
    let _ = CLI_RUNTIME.set(runtime);
    Ok(CLI_RUNTIME
        .get()
        .expect("CLI runtime should be initialized"))
}

fn with_auth<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<Request<T>> {
    let mut request = Request::new(message);
    if let Some(token) = &endpoint.token {
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))?,
        );
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
    if body.is_empty() {
        return vec![WriteFileRequest {
            path,
            data: Vec::new(),
        }];
    }
    body.chunks(64 * 1024)
        .enumerate()
        .map(|(index, chunk)| WriteFileRequest {
            path: if index == 0 {
                path.clone()
            } else {
                String::new()
            },
            data: chunk.to_vec(),
        })
        .collect()
}

fn chunk_stdin_requests(job_id: String, body: &[u8]) -> Vec<JobStdinRequest> {
    if body.is_empty() {
        return vec![JobStdinRequest {
            job_id,
            data: Vec::new(),
        }];
    }
    body.chunks(64 * 1024)
        .enumerate()
        .map(|(index, chunk)| JobStdinRequest {
            job_id: if index == 0 {
                job_id.clone()
            } else {
                String::new()
            },
            data: chunk.to_vec(),
        })
        .collect()
}

fn grpc_job_run_request(value: JobRunRequest) -> operon_protocol::runtime::v1::JobRunRequest {
    let has_timeout_secs = value.timeout_secs.is_some();
    operon_protocol::runtime::v1::JobRunRequest {
        command: value.command,
        cwd: value.cwd.unwrap_or_default(),
        timeout_secs: value.timeout_secs.unwrap_or_default(),
        secrets: value.secrets,
        has_timeout_secs,
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
    fn chunks_write_requests_with_path_only_on_first_chunk() {
        let chunks = chunk_write_requests("file.txt".to_string(), &[1_u8; 70 * 1024]);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].path, "file.txt");
        assert!(chunks[1].path.is_empty());
    }
}
