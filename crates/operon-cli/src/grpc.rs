use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use futures_util::stream;
use operon_core::{
    AuditLog, CapabilityDiagnosticRequest, CapabilityList, FsList, FsStat, FsWrite, HealthStatus,
    JobEvent, JobList, JobLogList, JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose,
    NodeInfo, PolicyDecision, RequestContext, ServiceCheck, ServiceList,
};
use operon_grpc_client::{chunk_stdin_requests, chunk_write_requests};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    job_log_stream_event, operon_runtime_client::OperonRuntimeClient,
    service_datagram_tunnel_request, service_datagram_tunnel_response, service_tunnel_request,
    service_tunnel_response, FsCopyRequest, FsListRequest, FsPathRequest, FsRenameRequest,
    FsTruncateRequest, GetNodeRequest, HealthRequest, JobCancelRequest, JobIdRequest,
    ListAuditRequest, ListCapabilitiesRequest, ListJobsRequest, ListServicesRequest,
    ServiceDatagram, ServiceDatagramTunnelRequest, ServiceDatagramTunnelTarget, ServiceIdRequest,
    ServiceTunnelClose, ServiceTunnelData, ServiceTunnelRequest, ServiceTunnelTarget,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::mpsc,
};
use tonic::transport::Channel;

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

pub async fn explain_capability(
    endpoint: &NodeEndpoint,
    request: CapabilityDiagnosticRequest,
) -> anyhow::Result<PolicyDecision> {
    call(endpoint, |mut client, endpoint| async move {
        client
            .explain_capability(with_auth(&endpoint, request.into())?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
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
    let mut entries = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let path = path.clone();
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_fs(with_auth(
                        &endpoint,
                        FsListRequest {
                            path,
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        entries.extend(response.entries.into_iter().map(Into::into));
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(FsList {
        path,
        entries,
        next_page_token: String::new(),
    })
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

pub async fn forward_service_connection(
    endpoint: &NodeEndpoint,
    service_id: &str,
    socket: TcpStream,
) -> anyhow::Result<()> {
    let service_id = service_id.to_string();
    let (mut local_reader, mut local_writer) = socket.into_split();
    call(endpoint, |mut client, endpoint| async move {
        let outbound = async_stream::stream! {
            yield ServiceTunnelRequest {
                payload: Some(service_tunnel_request::Payload::Target(ServiceTunnelTarget {
                    service_id,
                })),
            };
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                match local_reader.read(&mut buffer).await {
                    Ok(0) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Close(ServiceTunnelClose {
                                reason: "local client closed".to_string(),
                            })),
                        };
                        break;
                    }
                    Ok(bytes_read) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Data(ServiceTunnelData {
                                data: buffer[..bytes_read].to_vec(),
                            })),
                        };
                    }
                    Err(error) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Close(ServiceTunnelClose {
                                reason: format!("local client read failed: {error}"),
                            })),
                        };
                        break;
                    }
                }
            }
        };
        let mut inbound = client
            .open_service_tunnel(with_auth(&endpoint, outbound)?)
            .await?
            .into_inner();
        while let Some(message) = inbound.message().await? {
            match message.payload {
                Some(service_tunnel_response::Payload::Opened(_)) => {}
                Some(service_tunnel_response::Payload::Data(data)) => {
                    local_writer.write_all(&data.data).await?;
                }
                Some(service_tunnel_response::Payload::Close(_)) | None => break,
            }
        }
        let _ = local_writer.shutdown().await;
        Ok(())
    })
    .await
}

#[derive(Debug, Default)]
struct DatagramPeerState {
    next_peer_id: u64,
    addr_to_peer: BTreeMap<SocketAddr, String>,
    peer_to_addr: BTreeMap<String, SocketAddr>,
}

pub async fn forward_service_datagrams(
    endpoint: &NodeEndpoint,
    service_id: &str,
    socket: UdpSocket,
) -> anyhow::Result<()> {
    let service_id = service_id.to_string();
    let socket = Arc::new(socket);
    call(endpoint, |mut client, endpoint| async move {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        request_tx
            .send(ServiceDatagramTunnelRequest {
                payload: Some(service_datagram_tunnel_request::Payload::Target(
                    ServiceDatagramTunnelTarget { service_id },
                )),
            })
            .map_err(|_| anyhow::anyhow!("failed to open UDP datagram tunnel request stream"))?;

        let peer_state = Arc::new(Mutex::new(DatagramPeerState::default()));
        let local_reader = socket.clone();
        let local_request_tx = request_tx.clone();
        let local_peer_state = peer_state.clone();
        let local_read_task = tokio::spawn(async move {
            let mut buffer = vec![0_u8; 65_507];
            loop {
                let Ok((bytes_read, peer_addr)) = local_reader.recv_from(&mut buffer).await else {
                    break;
                };
                let peer_id = match datagram_peer_id(&local_peer_state, peer_addr) {
                    Ok(peer_id) => peer_id,
                    Err(_) => break,
                };
                if local_request_tx
                    .send(ServiceDatagramTunnelRequest {
                        payload: Some(service_datagram_tunnel_request::Payload::Datagram(
                            ServiceDatagram {
                                peer_id,
                                data: buffer[..bytes_read].to_vec(),
                            },
                        )),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        let outbound = async_stream::stream! {
            while let Some(request) = request_rx.recv().await {
                yield request;
            }
        };
        let request = match with_auth(&endpoint, outbound) {
            Ok(request) => request,
            Err(error) => {
                abort_and_wait(local_read_task).await;
                return Err(error);
            }
        };
        let response = match client.open_service_datagram_tunnel(request).await {
            Ok(response) => response,
            Err(error) => {
                abort_and_wait(local_read_task).await;
                return Err(error.into());
            }
        };
        let mut inbound = response.into_inner();
        while let Some(message) = inbound.message().await? {
            match message.payload {
                Some(service_datagram_tunnel_response::Payload::Opened(_)) => {}
                Some(service_datagram_tunnel_response::Payload::Datagram(datagram)) => {
                    let peer_addr = peer_addr_for_id(&peer_state, &datagram.peer_id)?;
                    if let Some(peer_addr) = peer_addr {
                        socket.send_to(&datagram.data, peer_addr).await?;
                    }
                }
                Some(service_datagram_tunnel_response::Payload::Close(close)) => {
                    if close.peer_id.is_empty() {
                        break;
                    }
                    remove_datagram_peer(&peer_state, &close.peer_id)?;
                }
                None => {}
            }
        }
        abort_and_wait(local_read_task).await;
        Ok(())
    })
    .await
}

async fn abort_and_wait<T>(task: tokio::task::JoinHandle<T>) {
    task.abort();
    let _ = task.await;
}

fn datagram_peer_id(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_addr: SocketAddr,
) -> anyhow::Result<String> {
    let mut state = peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?;
    if let Some(peer_id) = state.addr_to_peer.get(&peer_addr) {
        return Ok(peer_id.clone());
    }
    state.next_peer_id = state.next_peer_id.saturating_add(1);
    let peer_id = format!("peer-{}", state.next_peer_id);
    state.addr_to_peer.insert(peer_addr, peer_id.clone());
    state.peer_to_addr.insert(peer_id.clone(), peer_addr);
    Ok(peer_id)
}

fn peer_addr_for_id(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_id: &str,
) -> anyhow::Result<Option<SocketAddr>> {
    Ok(peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?
        .peer_to_addr
        .get(peer_id)
        .copied())
}

fn remove_datagram_peer(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_id: &str,
) -> anyhow::Result<()> {
    let mut state = peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?;
    if let Some(peer_addr) = state.peer_to_addr.remove(peer_id) {
        state.addr_to_peer.remove(&peer_addr);
    }
    Ok(())
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
    let client = operon_grpc_client::connect(&endpoint).await?;
    f(client, endpoint).await
}

fn with_auth<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<tonic::Request<T>> {
    if let Ok(context) = REQUEST_CONTEXT.try_with(Clone::clone) {
        return operon_grpc_client::request_with_context(endpoint, Some(&context), message);
    }
    operon_grpc_client::request(endpoint, message)
}

fn grpc_job_run_request(value: JobRunRequest) -> operon_protocol::runtime::v1::JobRunRequest {
    operon_protocol::runtime::v1::JobRunRequest {
        command: value.command,
        cwd: value.cwd.unwrap_or_default(),
        timeout_secs: value.timeout_secs,
        secrets: value.secrets,
        argv: value.argv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_grpc_uri_to_tonic_http_uri() {
        assert_eq!(
            operon_grpc_client::grpc_channel_uri("grpc://127.0.0.1:7789").expect("uri"),
            "http://127.0.0.1:7789"
        );
    }

    #[test]
    fn chunks_write_requests_use_target_then_data_chunks() {
        let chunks = chunk_write_requests("file.txt".to_string(), &[1_u8; 70 * 1024]);

        assert_eq!(chunks.len(), 3);
        assert!(matches!(
            chunks[0].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Target(target)) if target.path == "file.txt"
        ));
        assert!(matches!(
            chunks[1].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 64 * 1024
        ));
        assert!(matches!(
            chunks[2].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 6 * 1024
        ));
    }

    #[tokio::test]
    async fn with_auth_includes_execution_context_metadata() {
        let endpoint = NodeEndpoint {
            node_id: "node-a".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
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
