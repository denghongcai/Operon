use std::{
    collections::{BTreeMap, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use operon_config::{resolve_path, OperonConfig};
use operon_core::{
    AuditEvent, AuditLog, CapabilityList, FsEntry, FsList, FsStat, FsWrite, HealthStatus, JobEvent,
    JobList, JobLog, JobLogList, JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose,
    NodeInfo, PolicyConfig, RequestContext, RuntimeErrorKind, ServiceCheck, ServiceDefinition,
    ServiceList,
};
#[cfg(test)]
use operon_core::{FsMountPolicy, FsPermissions, FsPolicy, JobPolicy, ServicePolicy};
use operon_fs::{
    authorize_fs, join_virtual_path, resolve_create_workspace_path,
    resolve_existing_workspace_leaf_path, resolve_existing_workspace_path,
    resolve_write_workspace_path,
};
use operon_process::{authorize_job, job_environment, resolve_job_secrets};
use operon_protocol::runtime::v1::{
    job_stdin_request,
    operon_runtime_server::{OperonRuntime, OperonRuntimeServer},
    write_file_request, FileChunk, FsCopyRequest, FsPathRequest, FsRenameRequest,
    FsTruncateRequest, FsWriteRangeRequest, GetNodeRequest, HealthRequest, JobIdRequest,
    ListAuditRequest, ListCapabilitiesRequest, ListJobsRequest, ListServicesRequest,
    ServiceIdRequest, WriteFileRequest,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::{Child, Command as TokioCommand},
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
    time,
};
use tokio_util::io::ReaderStream;
use tonic::{metadata::MetadataMap, transport::Server, Request, Response as GrpcResponse, Status};

mod defaults;
mod grpc_status;
mod lan_advertise;
mod locks;
mod store_config;

use defaults::{default_capabilities, default_policy};
use grpc_status::{status_from_error, status_from_io_error};
use lan_advertise::advertise_lan;
use locks::lock;
use store_config::resolve_store_path;

const RUN_ID_METADATA: &str = "x-operon-run-id";
const STEP_ID_METADATA: &str = "x-operon-step-id";
const MAX_IN_MEMORY_AUDIT_EVENTS: usize = 10_000;
const MAX_IN_MEMORY_JOB_LOGS: usize = 10_000;
const MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS: usize = 512;
const MAX_FS_WRITE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const MAX_FS_FILE_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

tokio::task_local! {
    static AUDIT_CONTEXT: RequestContext;
}

type JobStdinSender = mpsc::UnboundedSender<Vec<u8>>;
type JobStdinRegistry = Arc<Mutex<BTreeMap<String, JobStdinSender>>>;
type JobEventSender = broadcast::Sender<JobEvent>;
type JobLogSender = broadcast::Sender<JobLog>;

#[derive(Debug, Default)]
struct JobLogBuffer {
    logs: VecDeque<JobLog>,
    next_sequence: u64,
    dropped_log_count: u64,
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

#[derive(Debug, Clone)]
struct AppState {
    node: NodeInfo,
    capabilities: CapabilityList,
    workspace: PathBuf,
    policy: PolicyConfig,
    auth_token: Option<String>,
    store: Option<PathBuf>,
    secrets: Arc<BTreeMap<String, String>>,
    audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    job_logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    job_events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    job_log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    job_cancel: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    job_stdin: JobStdinRegistry,
    next_job_id: Arc<AtomicU64>,
}

struct JobTask {
    audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    stdin: JobStdinRegistry,
    store: Option<PathBuf>,
    job_id: String,
    command: String,
    cwd: PathBuf,
    timeout_secs: u64,
    env: BTreeMap<String, String>,
    subject: String,
    node_id: String,
    audit_context: RequestContext,
    cancel_rx: oneshot::Receiver<()>,
    stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

struct JobCompletion {
    audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    stdin: JobStdinRegistry,
    store: Option<PathBuf>,
    job_id: String,
    subject: String,
    node_id: String,
    audit_context: RequestContext,
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
    let secrets_path = config
        .secrets
        .as_ref()
        .and_then(|secrets| secrets.file.as_ref())
        .map(|path| resolve_path(&config_dir, path));
    let secrets = load_secrets(secrets_path.as_deref())?;
    let stored_jobs = operon_store::load_jobs(store.as_deref())?;
    let next_job_id = next_job_sequence(&stored_jobs);
    let node = NodeInfo {
        id: daemon.node_id.clone(),
        hostname: hostname(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    };
    let capability_list = default_capabilities(&node.id);
    let jobs = Arc::new(Mutex::new(stored_jobs));
    let state = AppState {
        capabilities: capability_list.clone(),
        node,
        workspace: daemon.workspace,
        policy,
        auth_token,
        store: store.clone(),
        secrets: Arc::new(secrets),
        audit: Arc::new(Mutex::new(VecDeque::new())),
        jobs,
        job_logs: Arc::new(Mutex::new(BTreeMap::new())),
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

#[derive(Debug, Clone)]
struct GrpcRuntime {
    state: AppState,
}

type GrpcFileStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<FileChunk, Status>> + Send + 'static>>;
type GrpcJobLogStream = Pin<
    Box<
        dyn futures_util::Stream<Item = Result<operon_protocol::runtime::v1::JobLog, Status>>
            + Send
            + 'static,
    >,
>;
type GrpcJobEventStream = Pin<
    Box<
        dyn futures_util::Stream<Item = Result<operon_protocol::runtime::v1::JobEvent, Status>>
            + Send
            + 'static,
    >,
>;

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
                let stat = grpc_fs_stat(&self.state, path).await?;
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
                let list = grpc_fs_list(&self.state, path).await?;
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
                if let Err(error) = authorize_fs(&self.state.policy, "read", &path) {
                    record_audit(&self.state, "read-stream", &path, false, &error.1);
                    return Err(status_from_error(error));
                }
                let full_path = match resolve_existing_workspace_path(&self.state.workspace, &path)
                {
                    Ok(path) => path,
                    Err(error) => {
                        record_audit(&self.state, "read-stream", &path, false, &error.1);
                        return Err(status_from_error(error));
                    }
                };
                let file = tokio::fs::File::open(&full_path)
                    .await
                    .map_err(|error| Status::internal(error.to_string()))?;
                record_audit(&self.state, "read-stream", &path, true, "allowed");
                let stream = ReaderStream::new(file).map(|chunk| {
                    chunk
                        .map(|data| FileChunk {
                            data: data.to_vec(),
                        })
                        .map_err(|error| Status::internal(error.to_string()))
                });
                Ok(GrpcResponse::new(Box::pin(stream) as Self::ReadFileStream))
            })
            .await
    }

    async fn write_file(
        &self,
        request: Request<tonic::Streaming<WriteFileRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let mut path = None;
                let mut file = None;
                let mut bytes_written = 0_u64;

                while let Some(message) = stream.next().await {
                    let message = message?;
                    match message.payload {
                        Some(write_file_request::Payload::Target(target)) => {
                            if path.is_some() {
                                return Err(Status::invalid_argument(
                                    "write stream target metadata was sent more than once",
                                ));
                            }
                            if target.path.is_empty() {
                                return Err(Status::invalid_argument(
                                    "write stream target path is required",
                                ));
                            }
                            if let Err(error) =
                                authorize_fs(&self.state.policy, "write", &target.path)
                            {
                                record_audit(
                                    &self.state,
                                    "write-stream",
                                    &target.path,
                                    false,
                                    &error.1,
                                );
                                return Err(status_from_error(error));
                            }
                            let full_path = match resolve_write_workspace_path(
                                &self.state.workspace,
                                &target.path,
                            ) {
                                Ok(path) => path,
                                Err(error) => {
                                    record_audit(
                                        &self.state,
                                        "write-stream",
                                        &target.path,
                                        false,
                                        &error.1,
                                    );
                                    return Err(status_from_error(error));
                                }
                            };
                            if let Some(parent) = full_path.parent() {
                                tokio::fs::create_dir_all(parent)
                                    .await
                                    .map_err(|error| Status::internal(error.to_string()))?;
                            }
                            file = Some(
                                tokio::fs::File::create(&full_path)
                                    .await
                                    .map_err(|error| Status::internal(error.to_string()))?,
                            );
                            path = Some(target.path);
                        }
                        Some(write_file_request::Payload::Chunk(chunk)) => {
                            let Some(file) = &mut file else {
                                return Err(Status::invalid_argument(
                                    "write stream chunk arrived before target metadata",
                                ));
                            };
                            validate_write_chunk(chunk.data.len())?;
                            bytes_written =
                                checked_file_end(bytes_written, chunk.data.len(), "write stream")?;
                            file.write_all(&chunk.data)
                                .await
                                .map_err(|error| Status::internal(error.to_string()))?;
                        }
                        None => {
                            return Err(Status::invalid_argument(
                                "write stream message is missing payload",
                            ));
                        }
                    }
                }

                let Some(path) = path else {
                    return Err(Status::invalid_argument(
                        "write stream did not include target metadata",
                    ));
                };
                record_audit(&self.state, "write-stream", &path, true, "allowed");
                Ok(GrpcResponse::new(
                    FsWrite {
                        path,
                        bytes_written,
                    }
                    .into(),
                ))
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
                let write =
                    grpc_fs_write_range(&self.state, request.path, request.offset, request.data)
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
                let stat = grpc_fs_truncate(&self.state, request.path, request.size).await?;
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
                let stat = grpc_fs_mkdir(&self.state, path).await?;
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
                let path = grpc_fs_delete(&self.state, path).await?;
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
                grpc_fs_rename(&self.state, &request.from_path, &request.to_path).await?;
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
                    grpc_fs_copy(&self.state, &request.from_path, &request.to_path).await?;
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
        let initial_logs = job_log_list(&self.state, &job_id)?;
        let state = self.state.clone();
        let stream = async_stream::stream! {
            let mut next_sequence = 0;
            for log in initial_logs.logs {
                next_sequence = log.sequence.saturating_add(1);
                yield Ok::<_, Status>(log.into());
            }
            if !matches!(initial_record.status, JobStatus::Running) {
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
                                        yield Ok::<_, Status>(log.into());
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(_)) => {
                                    match job_log_list(&state, &job_id) {
                                        Ok(snapshot) => {
                                            for log in snapshot.logs {
                                                if log.sequence >= next_sequence {
                                                    next_sequence = log.sequence.saturating_add(1);
                                                    yield Ok::<_, Status>(log.into());
                                                }
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
                        event = event_receiver.recv() => {
                            match event {
                                Ok(event) => {
                                    if !matches!(event.status, JobStatus::Running) {
                                        match job_log_list(&state, &job_id) {
                                            Ok(snapshot) => {
                                                for log in snapshot.logs {
                                                    if log.sequence >= next_sequence {
                                                        next_sequence = log.sequence.saturating_add(1);
                                                        yield Ok::<_, Status>(log.into());
                                                    }
                                                }
                                            }
                                            Err(status) => {
                                                yield Err(status);
                                            }
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

fn paginate_items<T: Clone>(
    items: &[T],
    page_size: u32,
    page_token: &str,
) -> Result<(Vec<T>, String), Status> {
    let start = if page_token.is_empty() {
        0
    } else {
        page_token
            .parse::<usize>()
            .map_err(|_| Status::invalid_argument("invalid page_token"))?
    };
    if start > items.len() {
        return Err(Status::invalid_argument("page_token is out of range"));
    }
    if page_size == 0 {
        return Ok((items[start..].to_vec(), String::new()));
    }
    let size = usize::min(page_size as usize, 1000);
    let end = usize::min(start.saturating_add(size), items.len());
    let next = if end < items.len() {
        end.to_string()
    } else {
        String::new()
    };
    Ok((items[start..end].to_vec(), next))
}

fn validate_write_chunk(data_len: usize) -> Result<(), Status> {
    if data_len > MAX_FS_WRITE_CHUNK_BYTES {
        return Err(Status::invalid_argument(format!(
            "fs write chunk exceeds {} bytes",
            MAX_FS_WRITE_CHUNK_BYTES
        )));
    }
    Ok(())
}

fn checked_file_end(offset: u64, data_len: usize, operation: &str) -> Result<u64, Status> {
    let len = u64::try_from(data_len)
        .map_err(|_| Status::invalid_argument(format!("{operation} data length is too large")))?;
    let end = offset.checked_add(len).ok_or_else(|| {
        Status::invalid_argument(format!("{operation} offset plus data length overflows"))
    })?;
    if end > MAX_FS_FILE_BYTES {
        return Err(Status::invalid_argument(format!(
            "{operation} exceeds maximum fs object size of {} bytes",
            MAX_FS_FILE_BYTES
        )));
    }
    Ok(end)
}

fn authorize_grpc(state: &AppState, metadata: &MetadataMap) -> Result<RequestContext, Status> {
    if let Some(expected) = &state.auth_token {
        let Some(header) = metadata.get("authorization") else {
            return Err(Status::unauthenticated("missing bearer token"));
        };
        let Ok(header) = header.to_str() else {
            return Err(Status::unauthenticated("invalid bearer token"));
        };
        let Some(actual) = header.strip_prefix("Bearer ") else {
            return Err(Status::unauthenticated("invalid bearer token"));
        };
        if actual != expected {
            return Err(Status::unauthenticated("invalid bearer token"));
        }
    }

    Ok(RequestContext {
        run_id: metadata_value(metadata, RUN_ID_METADATA),
        step_id: metadata_value(metadata, STEP_ID_METADATA),
    })
}

fn metadata_value(metadata: &MetadataMap, key: &'static str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn grpc_fs_stat(state: &AppState, path: String) -> Result<FsStat, Status> {
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "stat", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "stat", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "stat", &path, true, "allowed");
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
    })
}

async fn grpc_fs_list(state: &AppState, path: String) -> Result<FsList, Status> {
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "list", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "list", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let mut entries = Vec::new();
    let mut reader = tokio::fs::read_dir(&full_path)
        .await
        .map_err(status_from_io_error)?;
    while let Some(entry) = reader.next_entry().await.map_err(status_from_io_error)? {
        let metadata = tokio::fs::symlink_metadata(entry.path())
            .await
            .map_err(status_from_io_error)?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child_path = join_virtual_path(&path, &name);
        entries.push(FsEntry {
            name,
            path: child_path,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    record_audit(state, "list", &path, true, "allowed");
    Ok(FsList { path, entries })
}

async fn grpc_fs_write_range(
    state: &AppState,
    path: String,
    offset: u64,
    data: Vec<u8>,
) -> Result<FsWrite, Status> {
    validate_write_chunk(data.len())?;
    checked_file_end(offset, data.len(), "write range")?;
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "write-range", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_write_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "write-range", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(status_from_io_error)?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(status_from_io_error)?;
    file.write_all(&data).await.map_err(status_from_io_error)?;
    file.flush().await.map_err(status_from_io_error)?;
    record_audit(state, "write-range", &path, true, "allowed");
    Ok(FsWrite {
        path,
        bytes_written: data.len() as u64,
    })
}

async fn grpc_fs_truncate(state: &AppState, path: String, size: u64) -> Result<FsStat, Status> {
    if size > MAX_FS_FILE_BYTES {
        return Err(Status::invalid_argument(format!(
            "truncate size exceeds maximum fs object size of {} bytes",
            MAX_FS_FILE_BYTES
        )));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "truncate", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_write_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "truncate", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(status_from_io_error)?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    file.set_len(size).await.map_err(status_from_io_error)?;
    record_audit(state, "truncate", &path, true, "allowed");
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
    })
}

async fn grpc_fs_mkdir(state: &AppState, path: String) -> Result<FsStat, Status> {
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "mkdir", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_create_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "mkdir", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    tokio::fs::create_dir_all(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "mkdir", &path, true, "allowed");
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
    })
}

async fn grpc_fs_delete(state: &AppState, path: String) -> Result<String, Status> {
    if let Err(error) = authorize_fs(&state.policy, "delete", &path) {
        record_audit(state, "delete", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_leaf_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "delete", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let metadata = tokio::fs::symlink_metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    if metadata.is_dir() {
        tokio::fs::remove_dir(&full_path)
            .await
            .map_err(status_from_io_error)?;
    } else {
        tokio::fs::remove_file(&full_path)
            .await
            .map_err(status_from_io_error)?;
    }
    record_audit(state, "delete", &path, true, "allowed");
    Ok(path)
}

async fn grpc_fs_rename(state: &AppState, from_path: &str, to_path: &str) -> Result<(), Status> {
    let resource = format!("{from_path} -> {to_path}");
    if let Err(error) = authorize_fs(&state.policy, "delete", from_path) {
        record_audit(state, "rename", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", to_path) {
        record_audit(state, "rename", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    let from_full_path = match resolve_existing_workspace_leaf_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "rename", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_write_workspace_path(&state.workspace, to_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "rename", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    tokio::fs::rename(&from_full_path, &to_full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "rename", &resource, true, "allowed");
    Ok(())
}

async fn grpc_fs_copy(state: &AppState, from_path: &str, to_path: &str) -> Result<u64, Status> {
    let resource = format!("{from_path} -> {to_path}");
    if let Err(error) = authorize_fs(&state.policy, "read", from_path) {
        record_audit(state, "copy", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", to_path) {
        record_audit(state, "copy", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    let from_full_path = match resolve_existing_workspace_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "copy", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_write_workspace_path(&state.workspace, to_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "copy", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let metadata = tokio::fs::metadata(&from_full_path)
        .await
        .map_err(status_from_io_error)?;
    if !metadata.is_file() {
        record_audit(state, "copy", &resource, false, "copy source is not a file");
        return Err(Status::failed_precondition("copy source is not a file"));
    }
    let bytes_copied = tokio::fs::copy(&from_full_path, &to_full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "copy", &resource, true, "allowed");
    Ok(bytes_copied)
}

fn start_job(state: &AppState, request: JobRunRequest) -> Result<JobRecord, Status> {
    let cwd_virtual = request.cwd.clone().unwrap_or_else(|| "/".to_string());
    if let Err(error) = authorize_job(&state.policy.job, &cwd_virtual, request.timeout_secs) {
        record_audit_capability(state, "job:default", "run", &cwd_virtual, false, &error.1);
        return Err(status_from_error(error));
    }
    let secret_env = match resolve_job_secrets(&state.policy.job, &state.secrets, &request.secrets)
    {
        Ok(secret_env) => secret_env,
        Err(error) => {
            record_audit_capability(state, "secret:default", "use", "*", false, &error.1);
            return Err(status_from_error(error));
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
    let store = state.store.clone();
    let command = request.command;
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
                    store,
                    job_id,
                    command,
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

fn get_job_record(state: &AppState, job_id: &str) -> Result<JobRecord, Status> {
    lock(&state.jobs, "job map")?
        .get(job_id)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("job `{job_id}` not found")))
}

async fn grpc_service_check(state: &AppState, service_id: String) -> Result<ServiceCheck, Status> {
    let service = match authorize_service(&state.policy, &service_id) {
        Ok(service) => service,
        Err(error) => {
            record_audit_capability(
                state,
                "service:default",
                "check",
                &service_id,
                false,
                &error.1,
            );
            return Err(status_from_error(error));
        }
    };

    let check =
        operon_network::check_tcp_service(&service, std::time::Duration::from_secs(2)).await;
    record_audit_capability(
        state,
        &format!("service:{}", service.id),
        "check",
        &service.id,
        check.ok,
        check.reason.as_deref().unwrap_or("reachable"),
    );

    Ok(check)
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

fn next_job_sequence(jobs: &BTreeMap<String, JobRecord>) -> u64 {
    jobs.keys()
        .filter_map(|id| job_sequence_number(id))
        .max()
        .unwrap_or(0)
        + 1
}

fn job_sequence_number(job_id: &str) -> Option<u64> {
    job_id.strip_prefix("job-")?.parse::<u64>().ok()
}

fn authorize_service(
    policy: &PolicyConfig,
    service_id: &str,
) -> Result<ServiceDefinition, (RuntimeErrorKind, String)> {
    policy
        .service
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| {
            (
                RuntimeErrorKind::Forbidden,
                format!("service `{service_id}` denied by policy"),
            )
        })
}

fn record_audit(state: &AppState, action: &str, resource: &str, allowed: bool, reason: &str) {
    record_audit_capability(state, "fs:workspace", action, resource, allowed, reason);
}

fn record_audit_capability(
    state: &AppState,
    capability: &str,
    action: &str,
    resource: &str,
    allowed: bool,
    reason: &str,
) {
    let context = current_request_context();
    let event = AuditEvent {
        subject: state.policy.subject.clone(),
        timestamp_ms: now_ms(),
        node_id: state.node.id.clone(),
        capability: capability.to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        allowed,
        reason: reason.to_string(),
        run_id: context.run_id,
        step_id: context.step_id,
    };
    push_audit_event(&state.audit, state.store.as_deref(), event);
}

fn push_audit_event(
    audit: &Arc<Mutex<VecDeque<AuditEvent>>>,
    store: Option<&Path>,
    event: AuditEvent,
) {
    let Ok(mut audit) = audit.lock() else {
        eprintln!("audit log mutex poisoned");
        return;
    };
    audit.push_back(event.clone());
    while audit.len() > MAX_IN_MEMORY_AUDIT_EVENTS {
        audit.pop_front();
    }
    drop(audit);
    append_store_record(
        store,
        &serde_json::json!({
            "kind": "audit",
            "event": event,
        }),
    );
}

fn current_request_context() -> RequestContext {
    AUDIT_CONTEXT.try_with(Clone::clone).unwrap_or_default()
}

fn append_store_record(path: Option<&Path>, record: &serde_json::Value) {
    operon_store::append_record(path, record);
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn run_job_task(task: JobTask) {
    let completion = JobCompletion {
        audit: task.audit.clone(),
        jobs: task.jobs.clone(),
        logs: task.logs.clone(),
        events: task.events.clone(),
        log_events: task.log_events.clone(),
        cancels: task.cancels.clone(),
        stdin: task.stdin.clone(),
        store: task.store.clone(),
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
                task.store.as_deref(),
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
            task.store.clone(),
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
            task.store.clone(),
            task.job_id.clone(),
            "stderr",
            stderr,
        )));
    }

    let (job_status, exit_code) = tokio::select! {
        status = child.wait() => job_status_from_wait(status, &task.jobs, &task.logs, &task.log_events, task.store.as_deref(), &task.job_id),
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
    store: Option<&Path>,
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
                store,
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
    let mut command = TokioCommand::new("/bin/sh");
    command
        .arg("-c")
        .arg(&task.command)
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

async fn terminate_child(child: &mut Child) {
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
async fn terminate_child_process_group(child: &mut Child) {
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
async fn terminate_direct_child(child: &mut Child) {
    if let Err(error) = child.start_kill() {
        tracing::warn!("failed to kill job process: {error}");
    }
    if let Err(error) = child.wait().await {
        tracing::warn!("failed to wait for killed job process: {error}");
    }
}

async fn wait_for_capture_tasks(capture_tasks: Vec<JoinHandle<()>>) {
    for task in capture_tasks {
        if let Err(error) = task.await {
            tracing::warn!("job stream capture task failed: {error}");
        }
    }
}

async fn capture_job_stream<R>(
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    store: Option<PathBuf>,
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
                store.as_deref(),
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
                    store.as_deref(),
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

async fn pump_job_stdin(
    mut receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(chunk) = receiver.recv().await {
        if stdin.write_all(&chunk).await.is_err() {
            break;
        }
    }
}

fn job_event_from_record(record: &JobRecord) -> JobEvent {
    JobEvent {
        job_id: record.id.clone(),
        status: record.status.clone(),
        exit_code: record.exit_code,
        log_count: record.log_count,
        logs_truncated: record.logs_truncated,
    }
}

fn job_log_list(state: &AppState, job_id: &str) -> Result<JobLogList, Status> {
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

fn append_job_log(
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    log_events: &Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    store: Option<&Path>,
    job_id: &str,
    mut log: JobLog,
) {
    let (log_count, logs_truncated, dropped_log_count, log) = {
        let Ok(mut buffers) = logs.lock() else {
            eprintln!("job log mutex poisoned");
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
        (
            buffer.next_sequence,
            buffer.dropped_log_count > 0,
            buffer.dropped_log_count,
            buffer.logs.back().expect("just pushed job log").clone(),
        )
    };

    if let Ok(mut jobs) = jobs.lock() {
        if let Some(record) = jobs.get_mut(job_id) {
            record.log_count = log_count;
            record.logs_truncated = logs_truncated;
        }
    } else {
        eprintln!("job map mutex poisoned");
    }
    match log_events.lock() {
        Ok(log_events) => {
            if let Some(sender) = log_events.get(job_id) {
                let _ = sender.send(log.clone());
            }
        }
        Err(_) => eprintln!("job log event mutex poisoned"),
    }
    append_store_record(
        store,
        &serde_json::json!({
            "kind": "job_log",
            "job_id": job_id,
            "log": log,
            "dropped_log_count": dropped_log_count,
        }),
    );
}

fn finish_job(completion: &JobCompletion, status: JobStatus, exit_code: Option<i32>) {
    if let Ok(mut cancels) = completion.cancels.lock() {
        cancels.remove(&completion.job_id);
    } else {
        eprintln!("job cancel mutex poisoned");
    }
    if let Ok(mut stdin) = completion.stdin.lock() {
        stdin.remove(&completion.job_id);
    } else {
        eprintln!("job stdin mutex poisoned");
    }

    let terminal = {
        let Ok(mut jobs) = completion.jobs.lock() else {
            eprintln!("job map mutex poisoned");
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
        append_store_record(
            completion.store.as_deref(),
            &serde_json::json!({
                "kind": "job",
                "record": record,
            }),
        );
        record_job_completion_audit(completion, &record);
        match completion.events.lock() {
            Ok(events) => {
                if let Some(sender) = events.get(&completion.job_id) {
                    let _ = sender.send(event);
                }
            }
            Err(_) => eprintln!("job event mutex poisoned"),
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
        completion.store.as_deref(),
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
        eprintln!("job event mutex poisoned");
    }
    if let Ok(mut log_events) = completion.log_events.lock() {
        log_events.remove(&completion.job_id);
    } else {
        eprintln!("job log event mutex poisoned");
    }
    prune_completed_job_log_buffers(&completion.jobs, &completion.logs);
}

fn prune_completed_job_log_buffers(
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    logs: &Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
) {
    let Ok(jobs) = jobs.lock() else {
        eprintln!("job map mutex poisoned");
        return;
    };
    let Ok(logs_guard) = logs.lock() else {
        eprintln!("job log mutex poisoned");
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
        Err(_) => eprintln!("job log mutex poisoned"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            capabilities: default_capabilities("node-a"),
            workspace,
            policy,
            auth_token: None,
            store: None,
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
        let chunk_error =
            validate_write_chunk(MAX_FS_WRITE_CHUNK_BYTES + 1).expect_err("chunk too large");
        assert_eq!(chunk_error.code(), tonic::Code::InvalidArgument);

        let overflow =
            checked_file_end(u64::MAX, 1, "write range").expect_err("offset should overflow");
        assert_eq!(overflow.code(), tonic::Code::InvalidArgument);

        let too_large_end = checked_file_end(MAX_FS_FILE_BYTES, 1, "write range")
            .expect_err("file bound should be enforced");
        assert_eq!(too_large_end.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn mkdir_creates_missing_parent_directories() {
        let base = unique_temp_dir("operond-mkdir-all-test");
        let workspace = base.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        let state = test_state(default_policy(), workspace.clone());

        grpc_fs_mkdir(&state, "/a/b/c".to_string())
            .await
            .expect("mkdir nested");

        assert!(workspace.join("a/b/c").is_dir());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn pagination_returns_deterministic_pages() {
        let items = vec![1, 2, 3, 4, 5];

        let (first, first_token) = paginate_items(&items, 2, "").expect("first page");
        assert_eq!(first, vec![1, 2]);
        assert_eq!(first_token, "2");

        let (second, second_token) = paginate_items(&items, 2, &first_token).expect("second page");
        assert_eq!(second, vec![3, 4]);
        assert_eq!(second_token, "4");

        let (last, last_token) = paginate_items(&items, 2, &second_token).expect("last page");
        assert_eq!(last, vec![5]);
        assert!(last_token.is_empty());
    }

    #[test]
    fn pagination_rejects_invalid_tokens() {
        let items = vec![1, 2, 3];

        assert_eq!(
            paginate_items(&items, 2, "not-a-number")
                .expect_err("invalid token")
                .code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            paginate_items(&items, 2, "4")
                .expect_err("out of range token")
                .code(),
            tonic::Code::InvalidArgument
        );
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
        let service = authorize_service(&test_policy(), "daemon").expect("service should resolve");

        assert_eq!(service.id, "daemon");
        assert_eq!(service.port, 7789);
    }

    #[test]
    fn authorize_service_rejects_unknown_service() {
        let error =
            authorize_service(&test_policy(), "missing").expect_err("service should be denied");

        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        assert!(error.1.contains("denied by policy"));
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

        grpc_fs_delete(&state, "/link".to_string())
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
            capabilities: default_capabilities("node-a"),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store: None,
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
            capabilities: default_capabilities("node-a"),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store: None,
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
            capabilities: default_capabilities("node-a"),
            workspace: PathBuf::from("/workspace"),
            policy: test_policy(),
            auth_token: None,
            store: None,
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
                None,
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
            store: None,
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
            store: None,
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
