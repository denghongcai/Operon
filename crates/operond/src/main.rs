#[cfg(test)]
use std::collections::VecDeque;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use operon_config::{resolve_path, OperonConfig};
use operon_core::{
    AuditLog, CapabilityList, HealthStatus, JobList, JobRunRequest, JobStatus, JobStdin,
    JobStdinClose, NodeInfo, RequestContext, ServiceList,
};
#[cfg(test)]
use operon_core::{
    FsMountPolicy, FsPermissions, FsPolicy, JobLog, JobPolicy, JobRecord, PolicyConfig,
    RuntimeErrorKind, ServiceDefinition, ServicePolicy,
};
#[cfg(test)]
use operon_fs::authorize_fs;
#[cfg(test)]
use operon_process::{authorize_job, resolve_job_secrets};
use operon_protocol::runtime::v1::{
    job_stdin_request,
    operon_runtime_server::{OperonRuntime, OperonRuntimeServer},
    FileChunk, FsCopyRequest, FsPathRequest, FsReadRangeRequest, FsRenameRequest,
    FsTruncateRequest, FsWriteRangeRequest, GetNodeRequest, HealthRequest, JobIdRequest,
    JobLogStreamEvent, ListAuditRequest, ListCapabilitiesRequest, ListJobsRequest,
    ListServicesRequest, ServiceDatagramTunnelRequest, ServiceIdRequest, ServiceTunnelRequest,
};
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response as GrpcResponse, Status};

mod audit;
mod auth;
mod defaults;
mod fs_service;
mod grpc_status;
mod job_runtime;
mod lan_advertise;
mod locks;
mod pagination;
mod service_forward;
mod state;
mod store_config;

#[cfg(test)]
use audit::now_ms;
use audit::{bounded_audit_events, record_audit, record_audit_capability};
use auth::authorize_grpc;
use defaults::{capabilities_from_policy, default_policy};
use job_runtime::{
    get_job_record, job_event_from_record, job_log_buffers_from_persisted_logs, job_log_complete,
    job_log_complete_event, job_log_entry_event, job_log_list, job_log_snapshot,
    job_log_snapshot_event, next_job_sequence, start_job,
};
use lan_advertise::advertise_lan;
use locks::lock;
use pagination::paginate_items;
#[cfg(test)]
use service_forward::authorize_service;
use service_forward::{grpc_service_check, open_service_datagram_tunnel, open_service_tunnel};
#[cfg(test)]
use state::MAX_IN_MEMORY_AUDIT_EVENTS;
use state::{AppState, JobEventSender, JobLogSender};
#[cfg(test)]
use state::{
    JobCompletion, JobLogBuffer, MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS, MAX_IN_MEMORY_JOB_LOGS,
};
use store_config::resolve_store_path;

pub(crate) const MAX_FS_WRITE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_FS_FILE_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_SERVICE_DATAGRAM_BYTES: usize = 65_507;
const SERVICE_DATAGRAM_PEER_IDLE_SECS: u64 = 60;

tokio::task_local! {
    static AUDIT_CONTEXT: RequestContext;
}

#[derive(Debug, Parser)]
#[command(name = "operond", about = "Operon capability daemon")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Start(StartArgs),
}

#[derive(Debug, Parser)]
struct StartArgs {
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Args::parse().command {
        Command::Start(args) => start(args).await,
    }
}

async fn start(args: StartArgs) -> anyhow::Result<()> {
    let config_path = args.config.unwrap_or_else(OperonConfig::default_path);
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    let daemon = config
        .daemon
        .clone()
        .ok_or_else(|| anyhow::anyhow!("config is missing daemon section"))?;
    let policy = config.policy.unwrap_or_else(default_policy);
    let auth_token = daemon.auth.resolve(&config_dir)?;
    let store = resolve_store_path(&config_dir, daemon.store.as_deref())?;
    let store_writer = operon_store::StoreWriter::new(store.clone());
    let secrets_path = config
        .secrets
        .as_ref()
        .and_then(|secrets| secrets.file.as_ref())
        .map(|path| resolve_path(&config_dir, path));
    let secrets = load_secrets(secrets_path.as_deref())?;
    let stored_jobs = operon_store::load_jobs(store.as_deref())?;
    let stored_audit_events = operon_store::load_audit_events(store.as_deref())?;
    let stored_job_logs = operon_store::load_job_logs(store.as_deref())?;
    let next_job_id = next_job_sequence(&stored_jobs);
    let node = NodeInfo {
        id: daemon.node_id.clone(),
        hostname: hostname(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    };
    let capability_list = capabilities_from_policy(&node.id, &policy);
    let jobs = Arc::new(Mutex::new(stored_jobs));
    let state = AppState {
        capabilities: capability_list.clone(),
        node,
        workspace: daemon.workspace,
        policy,
        auth_token,
        store_writer,
        secrets: Arc::new(secrets),
        audit: Arc::new(Mutex::new(bounded_audit_events(stored_audit_events))),
        jobs,
        job_logs: Arc::new(Mutex::new(job_log_buffers_from_persisted_logs(
            stored_job_logs,
        ))),
        job_events: Arc::new(Mutex::new(BTreeMap::new())),
        job_log_events: Arc::new(Mutex::new(BTreeMap::new())),
        job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
        job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
        next_job_id: Arc::new(AtomicU64::new(next_job_id)),
    };
    let mdns = if daemon.advertise_lan {
        Some(advertise_lan(
            &daemon.node_id,
            daemon.grpc_listen,
            &capability_list,
        )?)
    } else {
        None
    };

    tracing::info!("operond gRPC listening on {}", daemon.grpc_listen);
    Server::builder()
        .add_service(OperonRuntimeServer::new(GrpcRuntime { state }))
        .serve_with_shutdown(daemon.grpc_listen, shutdown_signal())
        .await?;

    drop(mdns);

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn load_secrets(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, String>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

#[derive(Debug, Clone)]
struct GrpcRuntime {
    state: AppState,
}

type GrpcFileStream = fs_service::FileStream;
type GrpcJobLogStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<JobLogStreamEvent, Status>> + Send + 'static>>;
type GrpcJobEventStream = Pin<
    Box<
        dyn futures_util::Stream<Item = Result<operon_protocol::runtime::v1::JobEvent, Status>>
            + Send
            + 'static,
    >,
>;
type GrpcServiceTunnelStream = service_forward::ServiceTunnelStream;
type GrpcServiceDatagramTunnelStream = service_forward::ServiceDatagramTunnelStream;

#[tonic::async_trait]
impl OperonRuntime for GrpcRuntime {
    async fn health(
        &self,
        request: Request<HealthRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::HealthStatus>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(
            HealthStatus {
                ok: true,
                node_id: self.state.node.id.clone(),
                version: operon_protocol::PROTOCOL_VERSION.to_string(),
            }
            .into(),
        ))
    }

    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::NodeInfo>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(self.state.node.clone().into()))
    }

    async fn list_capabilities(
        &self,
        request: Request<ListCapabilitiesRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::CapabilityList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (capabilities, next_page_token) = paginate_items(
            &self.state.capabilities.capabilities,
            request.page_size,
            &request.page_token,
        )?;
        let mut response: operon_protocol::runtime::v1::CapabilityList = CapabilityList {
            capabilities,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }

    async fn stat_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::stat(&self.state, path).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn list_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsList>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let list = fs_service::list(&self.state, path).await?;
                Ok(GrpcResponse::new(list.into()))
            })
            .await
    }

    type ReadFileStream = GrpcFileStream;

    async fn read_file(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<Self::ReadFileStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = fs_service::read_stream(&self.state, path).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    async fn read_file_range(
        &self,
        request: Request<FsReadRangeRequest>,
    ) -> Result<GrpcResponse<FileChunk>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let chunk =
                    fs_service::read_range(&self.state, request.path, request.offset, request.size)
                        .await?;
                Ok(GrpcResponse::new(chunk))
            })
            .await
    }

    async fn write_file(
        &self,
        request: Request<tonic::Streaming<operon_protocol::runtime::v1::WriteFileRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let write = fs_service::write_stream(&self.state, &mut stream).await?;
                Ok(GrpcResponse::new(write.into()))
            })
            .await
    }

    async fn write_file_range(
        &self,
        request: Request<FsWriteRangeRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let write = fs_service::write_range(
                    &self.state,
                    request.path,
                    request.offset,
                    request.data,
                )
                .await?;
                Ok(GrpcResponse::new(write.into()))
            })
            .await
    }

    async fn truncate_fs(
        &self,
        request: Request<FsTruncateRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::truncate(&self.state, request.path, request.size).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn mkdir_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::mkdir(&self.state, path).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn delete_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsDelete>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let path = fs_service::delete(&self.state, path).await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsDelete {
                    path,
                }))
            })
            .await
    }

    async fn rename_fs(
        &self,
        request: Request<FsRenameRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsRename>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                fs_service::rename(&self.state, &request.from_path, &request.to_path).await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsRename {
                    from_path: request.from_path,
                    to_path: request.to_path,
                }))
            })
            .await
    }

    async fn copy_fs(
        &self,
        request: Request<FsCopyRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsCopy>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let bytes_copied =
                    fs_service::copy(&self.state, &request.from_path, &request.to_path).await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsCopy {
                    from_path: request.from_path,
                    to_path: request.to_path,
                    bytes_copied,
                }))
            })
            .await
    }

    async fn run_job(
        &self,
        request: Request<operon_protocol::runtime::v1::JobRunRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobRecord>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let record = start_job(
                    &self.state,
                    JobRunRequest {
                        command: request.command,
                        argv: request.argv,
                        cwd: (!request.cwd.is_empty()).then_some(request.cwd),
                        timeout_secs: request.timeout_secs,
                        secrets: request.secrets,
                    },
                )?;
                Ok(GrpcResponse::new(record.into()))
            })
            .await
    }

    async fn get_job(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobRecord>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        let record = get_job_record(&self.state, &job_id)?;
        Ok(GrpcResponse::new(record.into()))
    }

    async fn list_jobs(
        &self,
        request: Request<ListJobsRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let jobs = lock(&self.state.jobs, "job map")?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let (jobs, next_page_token) =
            paginate_items(&jobs, request.page_size, &request.page_token)?;
        let mut response: operon_protocol::runtime::v1::JobList = JobList {
            jobs,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }

    type WatchJobStream = GrpcJobEventStream;

    async fn watch_job(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<Self::WatchJobStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        let mut receiver = lock(&self.state.job_events, "job event")?
            .get(&job_id)
            .map(JobEventSender::subscribe);
        let initial = job_event_from_record(&get_job_record(&self.state, &job_id)?);
        let state = self.state.clone();
        let stream = async_stream::stream! {
            let mut latest = initial;
            yield Ok::<_, Status>(latest.clone().into());
            if !matches!(latest.status, JobStatus::Running) {
                return;
            }
            if let Some(receiver) = receiver.as_mut() {
                loop {
                    match receiver.recv().await {
                        Ok(event) => {
                            latest = event;
                            yield Ok::<_, Status>(latest.clone().into());
                            if !matches!(latest.status, JobStatus::Running) {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            match get_job_record(&state, &job_id) {
                                Ok(record) => {
                                    latest = job_event_from_record(&record);
                                    yield Ok::<_, Status>(latest.clone().into());
                                    if !matches!(latest.status, JobStatus::Running) {
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
        Ok(GrpcResponse::new(Box::pin(stream)))
    }

    async fn list_job_logs(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobLogList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        get_job_record(&self.state, &job_id)?;
        Ok(GrpcResponse::new(
            job_log_list(&self.state, &job_id)?.into(),
        ))
    }

    type StreamJobLogsStream = GrpcJobLogStream;

    async fn stream_job_logs(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<Self::StreamJobLogsStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        let mut log_receiver = lock(&self.state.job_log_events, "job log event")?
            .get(&job_id)
            .map(JobLogSender::subscribe);
        let mut event_receiver = lock(&self.state.job_events, "job event")?
            .get(&job_id)
            .map(JobEventSender::subscribe);
        let initial_record = get_job_record(&self.state, &job_id)?;
        let (initial_snapshot, mut next_sequence) = job_log_snapshot(&self.state, &job_id)?;
        let state = self.state.clone();
        let stream = async_stream::stream! {
            yield Ok::<_, Status>(job_log_snapshot_event(initial_snapshot));
            if !matches!(initial_record.status, JobStatus::Running) {
                match job_log_complete(&state, &job_id) {
                    Ok(complete) => yield Ok(job_log_complete_event(complete)),
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
                                        yield Ok::<_, Status>(job_log_entry_event(&job_id, log));
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(_)) => {
                                    match job_log_snapshot(&state, &job_id) {
                                        Ok((snapshot, snapshot_next_sequence)) => {
                                            next_sequence = next_sequence.max(snapshot_next_sequence);
                                            yield Ok::<_, Status>(job_log_snapshot_event(snapshot));
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
                                    if !matches!(event.status, JobStatus::Running) {
                                        match job_log_snapshot(&state, &job_id) {
                                            Ok((snapshot, _snapshot_next_sequence)) => {
                                                yield Ok::<_, Status>(job_log_snapshot_event(snapshot));
                                            }
                                            Err(status) => {
                                                yield Err(status);
                                            }
                                        }
                                        match job_log_complete(&state, &job_id) {
                                            Ok(complete) => yield Ok::<_, Status>(job_log_complete_event(complete)),
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
        Ok(GrpcResponse::new(Box::pin(stream)))
    }

    async fn write_job_stdin(
        &self,
        request: Request<tonic::Streaming<operon_protocol::runtime::v1::JobStdinRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobStdin>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        let mut job_id = None;
        let mut sender = None;
        let mut bytes_written = 0_u64;
        while let Some(message) = stream.next().await {
            let message = message?;
            match message.payload {
                Some(job_stdin_request::Payload::Target(target)) => {
                    if job_id.is_some() {
                        return Err(Status::invalid_argument(
                            "stdin stream target metadata was sent more than once",
                        ));
                    }
                    if target.job_id.is_empty() {
                        return Err(Status::invalid_argument(
                            "stdin stream target job_id is required",
                        ));
                    }
                    sender = Some(
                        lock(&self.state.job_stdin, "job stdin")?
                            .get(&target.job_id)
                            .cloned()
                            .ok_or_else(|| {
                                Status::not_found(format!(
                                    "job `{}` has no open stdin",
                                    target.job_id
                                ))
                            })?,
                    );
                    job_id = Some(target.job_id);
                }
                Some(job_stdin_request::Payload::Chunk(chunk)) => {
                    let Some(sender) = &sender else {
                        return Err(Status::invalid_argument(
                            "stdin stream chunk arrived before target metadata",
                        ));
                    };
                    bytes_written += chunk.data.len() as u64;
                    sender
                        .send(chunk.data)
                        .map_err(|_| Status::failed_precondition("job stdin is closed"))?;
                }
                None => {
                    return Err(Status::invalid_argument(
                        "stdin stream message is missing payload",
                    ));
                }
            }
        }
        let Some(job_id) = job_id else {
            return Err(Status::invalid_argument(
                "stdin stream did not include target metadata",
            ));
        };
        Ok(GrpcResponse::new(
            JobStdin {
                job_id,
                bytes_written,
            }
            .into(),
        ))
    }

    async fn close_job_stdin(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobStdinClose>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        let closed = lock(&self.state.job_stdin, "job stdin")?
            .remove(&job_id)
            .is_some();
        Ok(GrpcResponse::new(JobStdinClose { job_id, closed }.into()))
    }

    async fn cancel_job(
        &self,
        request: Request<operon_protocol::runtime::v1::JobCancelRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobRecord>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        AUDIT_CONTEXT
            .scope(context, async {
                if let Some(sender) = lock(&self.state.job_cancel, "job cancel")?.remove(&job_id) {
                    let _ = sender.send(());
                    record_audit_capability(
                        &self.state,
                        "job:default",
                        "cancel",
                        &job_id,
                        true,
                        "cancel requested",
                    );
                }
                let record = get_job_record(&self.state, &job_id)?;
                Ok(GrpcResponse::new(record.into()))
            })
            .await
    }

    async fn list_services(
        &self,
        request: Request<ListServicesRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (services, next_page_token) = paginate_items(
            &self.state.policy.service.services,
            request.page_size,
            &request.page_token,
        )?;
        let mut response: operon_protocol::runtime::v1::ServiceList = ServiceList {
            services,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }

    async fn check_service(
        &self,
        request: Request<ServiceIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceCheck>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let service_id = request.into_inner().service_id;
        AUDIT_CONTEXT
            .scope(context, async {
                let check = grpc_service_check(&self.state, service_id).await?;
                Ok(GrpcResponse::new(check.into()))
            })
            .await
    }

    type OpenServiceTunnelStream = GrpcServiceTunnelStream;

    async fn open_service_tunnel(
        &self,
        request: Request<tonic::Streaming<ServiceTunnelRequest>>,
    ) -> Result<GrpcResponse<Self::OpenServiceTunnelStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let input = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = open_service_tunnel(&self.state, input).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    type OpenServiceDatagramTunnelStream = GrpcServiceDatagramTunnelStream;

    async fn open_service_datagram_tunnel(
        &self,
        request: Request<tonic::Streaming<ServiceDatagramTunnelRequest>>,
    ) -> Result<GrpcResponse<Self::OpenServiceDatagramTunnelStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let input = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = open_service_datagram_tunnel(&self.state, input).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    async fn list_audit(
        &self,
        request: Request<ListAuditRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::AuditLog>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let events = lock(&self.state.audit, "audit log")?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let (events, next_page_token) =
            paginate_items(&events, request.page_size, &request.page_token)?;
        let mut response: operon_protocol::runtime::v1::AuditLog = AuditLog {
            events,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_runtime::{append_job_log, finish_job, start_job};

    fn test_policy() -> PolicyConfig {
        PolicyConfig {
            subject: "test-subject".to_string(),
            fs: FsPolicy {
                mounts: vec![FsMountPolicy {
                    name: "workspace".to_string(),
                    path: "/workspace".to_string(),
                    permissions: FsPermissions {
                        read: true,
                        write: false,
                        delete: false,
                    },
                }],
            },
            job: JobPolicy {
                allowed_cwds: vec!["/workspace".to_string()],
                default_timeout_secs: 10,
                max_timeout_secs: 30,
                preserve_env: false,
                env_allowlist: Vec::new(),
                allowed_secrets: vec!["TEST_SECRET".to_string()],
            },
            service: ServicePolicy {
                services: vec![ServiceDefinition {
                    id: "daemon".to_string(),
                    name: "daemon".to_string(),
                    host: "127.0.0.1".to_string(),
                    port: 7789,
                    protocol: operon_core::ServiceProtocol::Tcp,
                    description: "local daemon".to_string(),
                    permissions: operon_core::ServicePermissions::default(),
                }],
            },
        }
    }

    fn test_state(policy: PolicyConfig, workspace: PathBuf) -> AppState {
        AppState {
            node: NodeInfo {
                id: "node-a".to_string(),
                hostname: "host".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            capabilities: capabilities_from_policy("node-a", &test_policy()),
            workspace,
            policy,
            auth_token: None,
            store_writer: operon_store::StoreWriter::new(None),
            secrets: Arc::new(BTreeMap::new()),
            audit: Arc::new(Mutex::new(VecDeque::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_logs: Arc::new(Mutex::new(BTreeMap::new())),
            job_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}-{}", std::process::id(), now_ms()))
    }

    #[test]
    fn fs_range_validation_rejects_overflow_and_large_chunks() {
        let chunk_error = fs_service::validate_write_chunk(MAX_FS_WRITE_CHUNK_BYTES + 1)
            .expect_err("chunk too large");
        assert_eq!(chunk_error.code(), tonic::Code::InvalidArgument);

        let read_error =
            fs_service::validate_read_range_size((MAX_FS_WRITE_CHUNK_BYTES + 1) as u32)
                .expect_err("read range too large");
        assert_eq!(read_error.code(), tonic::Code::InvalidArgument);

        let overflow = fs_service::checked_file_end(u64::MAX, 1, "write range")
            .expect_err("offset should overflow");
        assert_eq!(overflow.code(), tonic::Code::InvalidArgument);

        let too_large_end = fs_service::checked_file_end(MAX_FS_FILE_BYTES, 1, "write range")
            .expect_err("file bound should be enforced");
        assert_eq!(too_large_end.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn read_range_reads_only_requested_bytes() {
        let base = unique_temp_dir("operond-read-range-test");
        let workspace = base.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(workspace.join("data.bin"), b"0123456789").expect("file");
        let state = test_state(default_policy(), workspace);

        let chunk = fs_service::read_range(&state, "/data.bin".to_string(), 3, 4)
            .await
            .expect("read range");

        assert_eq!(chunk.data, b"3456");
        let _ = fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn mkdir_creates_missing_parent_directories() {
        let base = unique_temp_dir("operond-mkdir-all-test");
        let workspace = base.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        let state = test_state(default_policy(), workspace.clone());

        fs_service::mkdir(&state, "/a/b/c".to_string())
            .await
            .expect("mkdir nested");

        assert!(workspace.join("a/b/c").is_dir());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn resolves_virtual_paths_under_workspace() {
        let resolved =
            operon_fs::resolve_workspace_path(Path::new("/srv/operon"), "/nested/file.txt")
                .expect("path should resolve");

        assert_eq!(resolved, PathBuf::from("/srv/operon/nested/file.txt"));
    }

    #[test]
    fn rejects_parent_dir_workspace_escape() {
        let error = operon_fs::resolve_workspace_path(Path::new("/srv/operon"), "/../etc/passwd")
            .expect_err("path escape should be rejected");

        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        assert!(error.1.contains("escapes workspace"));
    }

    #[test]
    fn policy_scope_matches_exact_path_and_children_only() {
        assert!(operon_fs::path_in_policy_scope("/workspace", "/workspace"));
        assert!(operon_fs::path_in_policy_scope(
            "/workspace/project",
            "/workspace"
        ));
        assert!(!operon_fs::path_in_policy_scope(
            "/workspace-other",
            "/workspace"
        ));
    }

    #[test]
    fn authorize_service_returns_allowed_service() {
        let service =
            authorize_service(&test_policy(), "daemon", "check").expect("service should resolve");

        assert_eq!(service.id, "daemon");
        assert_eq!(service.port, 7789);
    }

    #[test]
    fn authorize_service_rejects_unknown_service() {
        let error = authorize_service(&test_policy(), "missing", "check")
            .expect_err("service should be denied");

        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        assert!(error.1.contains("denied by policy"));
    }

    #[test]
    fn authorize_service_enforces_action_permissions() {
        let mut policy = test_policy();
        policy.service.services[0].permissions.forward = false;

        let error = authorize_service(&policy, "daemon", "forward")
            .expect_err("service forward should be denied");

        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        assert!(error.1.contains("action `forward` denied"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_removes_leaf_symlink_not_target() {
        let base = std::env::temp_dir().join(format!(
            "operond-delete-link-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&outside).expect("outside");
        let target = outside.join("secret.txt");
        fs::write(&target, "secret").expect("target");
        let link = workspace.join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let mut policy = default_policy();
        policy.fs.mounts[0].permissions.delete = true;
        let state = test_state(policy, workspace);

        fs_service::delete(&state, "/link".to_string())
            .await
            .expect("delete symlink");

        assert!(!link.exists());
        assert_eq!(
            fs::read_to_string(target).expect("target still exists"),
            "secret"
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn fs_policy_enforces_mount_permissions() {
        let policy = test_policy();

        assert!(authorize_fs(&policy, "read", "/workspace/file.txt").is_ok());

        let write_error = authorize_fs(&policy, "write", "/workspace/file.txt")
            .expect_err("write should be denied");
        assert_eq!(write_error.0, RuntimeErrorKind::Forbidden);

        let outside_error =
            authorize_fs(&policy, "read", "/outside/file.txt").expect_err("outside mount");
        assert_eq!(outside_error.1, "path is outside allowed fs mounts");
    }

    #[test]
    fn job_policy_enforces_cwd_and_timeout() {
        let policy = test_policy();

        assert!(authorize_job(&policy.job, "/workspace/project", Some(30)).is_ok());

        let cwd_error =
            authorize_job(&policy.job, "/tmp", Some(1)).expect_err("cwd should be denied");
        assert_eq!(cwd_error.1, "job cwd denied by policy");

        let timeout_error = authorize_job(&policy.job, "/workspace", Some(31))
            .expect_err("timeout should be denied");
        assert!(timeout_error.1.contains("exceeds policy maximum"));
    }

    #[test]
    fn denied_job_policy_audit_uses_reason_code() {
        let state = test_state(test_policy(), PathBuf::from("/workspace"));

        let error = start_job(
            &state,
            JobRunRequest {
                command: "pwd".to_string(),
                argv: Vec::new(),
                cwd: Some("/tmp".to_string()),
                timeout_secs: Some(1),
                secrets: Vec::new(),
            },
        )
        .expect_err("job cwd should be denied");

        assert_eq!(error.code(), tonic::Code::PermissionDenied);
        let audit = state.audit.lock().expect("audit");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].capability, "job:default");
        assert_eq!(audit[0].action, "run");
        assert_eq!(audit[0].resource, "/tmp");
        assert!(!audit[0].allowed);
        assert_eq!(audit[0].reason, "job-cwd-denied: job cwd denied by policy");
    }

    #[test]
    fn job_secrets_must_be_allowed_and_defined() {
        let mut secrets = BTreeMap::new();
        secrets.insert("TEST_SECRET".to_string(), "secret-value".to_string());
        let state = AppState {
            node: NodeInfo {
                id: "node-a".to_string(),
                hostname: "host".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            capabilities: capabilities_from_policy("node-a", &test_policy()),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store_writer: operon_store::StoreWriter::new(None),
            secrets: Arc::new(secrets),
            audit: Arc::new(Mutex::new(VecDeque::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_logs: Arc::new(Mutex::new(BTreeMap::new())),
            job_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        };

        let resolved = resolve_job_secrets(
            &state.policy.job,
            &state.secrets,
            &["TEST_SECRET".to_string()],
        )
        .expect("secret");
        assert_eq!(
            resolved.get("TEST_SECRET").map(String::as_str),
            Some("secret-value")
        );

        let denied = resolve_job_secrets(
            &state.policy.job,
            &state.secrets,
            &["DENIED_SECRET".to_string()],
        )
        .expect_err("denied secret should fail");
        assert_eq!(denied.0, RuntimeErrorKind::Forbidden);
    }

    #[tokio::test]
    async fn audit_event_uses_policy_subject_capability_and_context() {
        let state = AppState {
            node: NodeInfo {
                id: "node-a".to_string(),
                hostname: "host".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            capabilities: capabilities_from_policy("node-a", &test_policy()),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store_writer: operon_store::StoreWriter::new(None),
            secrets: Arc::new(BTreeMap::new()),
            audit: Arc::new(Mutex::new(VecDeque::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_logs: Arc::new(Mutex::new(BTreeMap::new())),
            job_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        };

        AUDIT_CONTEXT
            .scope(
                RequestContext {
                    run_id: Some("run-1".to_string()),
                    step_id: Some("step-1".to_string()),
                },
                async {
                    record_audit_capability(
                        &state,
                        "fs:workspace",
                        "read",
                        "/file.txt",
                        true,
                        "allowed",
                    );
                },
            )
            .await;

        let events = state.audit.lock().expect("audit log");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].subject, "test-subject");
        assert_eq!(events[0].node_id, "node-a");
        assert_eq!(events[0].capability, "fs:workspace");
        assert_eq!(events[0].run_id.as_deref(), Some("run-1"));
        assert_eq!(events[0].step_id.as_deref(), Some("step-1"));
        assert!(events[0].allowed);
    }

    #[test]
    fn audit_log_is_bounded_in_memory() {
        let state = AppState {
            node: NodeInfo {
                id: "node-a".to_string(),
                hostname: "host".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            capabilities: capabilities_from_policy("node-a", &test_policy()),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store_writer: operon_store::StoreWriter::new(None),
            secrets: Arc::new(BTreeMap::new()),
            audit: Arc::new(Mutex::new(VecDeque::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_logs: Arc::new(Mutex::new(BTreeMap::new())),
            job_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        };

        for index in 0..(MAX_IN_MEMORY_AUDIT_EVENTS + 5) {
            record_audit_capability(
                &state,
                "fs:workspace",
                "read",
                &format!("/file-{index}.txt"),
                true,
                "allowed",
            );
        }

        let events = state.audit.lock().expect("audit log");
        assert_eq!(events.len(), MAX_IN_MEMORY_AUDIT_EVENTS);
        assert_eq!(events[0].resource, "/file-5.txt");
    }

    #[test]
    fn job_logs_are_separate_and_bounded() {
        let jobs = Arc::new(Mutex::new(BTreeMap::from([(
            "job-1".to_string(),
            JobRecord {
                id: "job-1".to_string(),
                node_id: "node-a".to_string(),
                command: "echo test".to_string(),
                cwd: "/workspace".to_string(),
                status: JobStatus::Running,
                exit_code: None,
                log_count: 0,
                logs_truncated: false,
            },
        )])));
        let logs = Arc::new(Mutex::new(BTreeMap::from([(
            "job-1".to_string(),
            JobLogBuffer::default(),
        )])));
        let (sender, _) = broadcast::channel(1);
        let log_events = Arc::new(Mutex::new(BTreeMap::from([("job-1".to_string(), sender)])));

        for index in 0..(MAX_IN_MEMORY_JOB_LOGS + 5) {
            append_job_log(
                &jobs,
                &logs,
                &log_events,
                &operon_store::StoreWriter::new(None),
                "job-1",
                JobLog {
                    stream: "stdout".to_string(),
                    data: format!("line {index}").into_bytes(),
                    sequence: 0,
                },
            );
        }

        let record = jobs
            .lock()
            .expect("job map")
            .get("job-1")
            .expect("job")
            .clone();
        assert_eq!(record.log_count, (MAX_IN_MEMORY_JOB_LOGS + 5) as u64);
        assert!(record.logs_truncated);

        let buffers = logs.lock().expect("job logs");
        let buffer = buffers.get("job-1").expect("job log buffer");
        assert_eq!(buffer.logs.len(), MAX_IN_MEMORY_JOB_LOGS);
        assert_eq!(buffer.logs.front().expect("first retained").sequence, 5);
    }

    #[test]
    fn finished_job_runtime_state_is_cleaned_and_log_buffers_are_bounded() {
        let mut job_records = BTreeMap::new();
        let mut log_buffers = BTreeMap::new();
        for index in 1..=(MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS + 2) {
            let job_id = format!("job-{index}");
            job_records.insert(
                job_id.clone(),
                JobRecord {
                    id: job_id.clone(),
                    node_id: "node-a".to_string(),
                    command: "true".to_string(),
                    cwd: "/workspace".to_string(),
                    status: JobStatus::Succeeded,
                    exit_code: Some(0),
                    log_count: 0,
                    logs_truncated: false,
                },
            );
            log_buffers.insert(job_id, JobLogBuffer::default());
        }
        let target_job_id = format!("job-{}", MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS + 2);
        if let Some(record) = job_records.get_mut(&target_job_id) {
            record.status = JobStatus::Running;
            record.exit_code = None;
        }

        let jobs = Arc::new(Mutex::new(job_records));
        let logs = Arc::new(Mutex::new(log_buffers));
        let (event_sender, _) = broadcast::channel(1);
        let (log_sender, _) = broadcast::channel(1);
        let events = Arc::new(Mutex::new(BTreeMap::from([(
            target_job_id.clone(),
            event_sender,
        )])));
        let log_events = Arc::new(Mutex::new(BTreeMap::from([(
            target_job_id.clone(),
            log_sender,
        )])));
        let completion = JobCompletion {
            audit: Arc::new(Mutex::new(VecDeque::new())),
            jobs,
            logs: logs.clone(),
            events: events.clone(),
            log_events: log_events.clone(),
            cancels: Arc::new(Mutex::new(BTreeMap::new())),
            stdin: Arc::new(Mutex::new(BTreeMap::new())),
            store_writer: operon_store::StoreWriter::new(None),
            job_id: target_job_id.clone(),
            subject: "test-subject".to_string(),
            node_id: "node-a".to_string(),
            audit_context: RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
        };

        finish_job(&completion, JobStatus::Succeeded, Some(0));

        assert!(!events.lock().expect("events").contains_key(&target_job_id));
        assert!(!log_events
            .lock()
            .expect("log events")
            .contains_key(&target_job_id));
        let logs = logs.lock().expect("logs");
        assert_eq!(logs.len(), MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS);
        assert!(!logs.contains_key("job-1"));
        assert!(!logs.contains_key("job-2"));
        assert!(logs.contains_key(&target_job_id));
    }

    #[test]
    fn finish_job_records_terminal_audit_event() {
        let job_id = "job-1".to_string();
        let jobs = Arc::new(Mutex::new(BTreeMap::from([(
            job_id.clone(),
            JobRecord {
                id: job_id.clone(),
                node_id: "node-a".to_string(),
                command: "true".to_string(),
                cwd: "/".to_string(),
                status: JobStatus::Running,
                exit_code: None,
                log_count: 0,
                logs_truncated: false,
            },
        )])));
        let logs = Arc::new(Mutex::new(BTreeMap::new()));
        let (event_sender, _) = broadcast::channel(1);
        let (log_sender, _) = broadcast::channel(1);
        let audit = Arc::new(Mutex::new(VecDeque::new()));
        let completion = JobCompletion {
            audit: audit.clone(),
            jobs,
            logs,
            events: Arc::new(Mutex::new(BTreeMap::from([(job_id.clone(), event_sender)]))),
            log_events: Arc::new(Mutex::new(BTreeMap::from([(job_id.clone(), log_sender)]))),
            cancels: Arc::new(Mutex::new(BTreeMap::new())),
            stdin: Arc::new(Mutex::new(BTreeMap::new())),
            store_writer: operon_store::StoreWriter::new(None),
            job_id: job_id.clone(),
            subject: "test-subject".to_string(),
            node_id: "node-a".to_string(),
            audit_context: RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
        };

        finish_job(&completion, JobStatus::Failed, Some(7));

        let events = audit.lock().expect("audit");
        let event = events.back().expect("completion audit");
        assert_eq!(event.capability, "job:default");
        assert_eq!(event.action, "finish");
        assert_eq!(event.resource, job_id);
        assert!(event.allowed);
        assert_eq!(event.reason, "status=failed exit_code=7");
        assert_eq!(event.run_id.as_deref(), Some("run-1"));
        assert_eq!(event.step_id.as_deref(), Some("step-1"));
    }
}
