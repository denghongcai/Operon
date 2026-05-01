use std::{
    collections::BTreeMap,
    env, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use operon_core::{
    AuditEvent, AuditLog, Capability, CapabilityKind, CapabilityList, FsEntry, FsList,
    FsMountPolicy, FsPermissions, FsPolicy, FsStat, FsWrite, HealthStatus, JobList, JobLog,
    JobPolicy, JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose, NodeInfo,
    PolicyConfig, ServiceCheck, ServiceDefinition, ServiceList, ServicePolicy,
};
use operon_protocol::runtime::v1::{
    operon_runtime_server::{OperonRuntime, OperonRuntimeServer},
    FileChunk, FsCopyRequest, FsPathRequest, FsRenameRequest, FsTruncateRequest,
    FsWriteRangeRequest, GetNodeRequest, HealthRequest, JobIdRequest, ListAuditRequest,
    ListCapabilitiesRequest, ListJobsRequest, ListServicesRequest, ServiceIdRequest,
    WriteFileRequest,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command as TokioCommand,
    sync::{mpsc, oneshot},
    time,
};
use tokio_util::io::ReaderStream;
use tonic::{metadata::MetadataMap, transport::Server, Request, Response as GrpcResponse, Status};

const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

type JobStdinSender = mpsc::UnboundedSender<Vec<u8>>;
type JobStdinRegistry = Arc<Mutex<BTreeMap<String, JobStdinSender>>>;

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
    #[arg(long, default_value = "127.0.0.1:7789")]
    grpc_listen: SocketAddr,

    #[arg(long, default_value = "local")]
    node_id: String,

    #[arg(long, default_value = "/workspace")]
    workspace: PathBuf,

    #[arg(long)]
    policy: Option<PathBuf>,

    #[arg(long)]
    auth_token: Option<String>,

    #[arg(long)]
    auth_token_file: Option<PathBuf>,

    #[arg(long)]
    store: Option<PathBuf>,

    #[arg(long)]
    secrets: Option<PathBuf>,

    #[arg(long)]
    advertise_lan: bool,
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
    audit: Arc<Mutex<Vec<AuditEvent>>>,
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    job_cancel: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    job_stdin: JobStdinRegistry,
    next_job_id: Arc<AtomicU64>,
}

struct JobTask {
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    stdin: JobStdinRegistry,
    store: Option<PathBuf>,
    job_id: String,
    command: String,
    cwd: PathBuf,
    timeout_secs: u64,
    env: BTreeMap<String, String>,
    cancel_rx: oneshot::Receiver<()>,
    stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeErrorKind {
    Forbidden,
    NotFound,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Args::parse().command {
        Command::Start(args) => start(args).await,
    }
}

async fn start(args: StartArgs) -> anyhow::Result<()> {
    let policy = load_policy(args.policy.as_deref())?;
    let auth_token = load_auth_token(args.auth_token, args.auth_token_file.as_deref())?;
    let secrets = load_secrets(args.secrets.as_deref())?;
    let stored_jobs = load_store_jobs(args.store.as_deref())?;
    let next_job_id = next_job_sequence(&stored_jobs);
    let node = NodeInfo {
        id: args.node_id.clone(),
        hostname: hostname(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    };
    let capability_list = default_capabilities(&node.id);
    let jobs = Arc::new(Mutex::new(stored_jobs));
    let state = AppState {
        capabilities: capability_list.clone(),
        node,
        workspace: args.workspace,
        policy,
        auth_token,
        store: args.store.clone(),
        secrets: Arc::new(secrets),
        audit: Arc::new(Mutex::new(Vec::new())),
        jobs,
        job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
        job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
        next_job_id: Arc::new(AtomicU64::new(next_job_id)),
    };
    let mdns = if args.advertise_lan {
        Some(advertise_lan(
            &args.node_id,
            args.grpc_listen,
            &capability_list,
        )?)
    } else {
        None
    };

    tracing::info!("operond gRPC listening on {}", args.grpc_listen);
    Server::builder()
        .add_service(OperonRuntimeServer::new(GrpcRuntime { state }))
        .serve_with_shutdown(args.grpc_listen, shutdown_signal())
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
                version: env!("CARGO_PKG_VERSION").to_string(),
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
        Ok(GrpcResponse::new(self.state.capabilities.clone().into()))
    }

    async fn stat_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        let stat = grpc_fs_stat(&self.state, path).await?;
        Ok(GrpcResponse::new(stat.into()))
    }

    async fn list_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        let list = grpc_fs_list(&self.state, path).await?;
        Ok(GrpcResponse::new(list.into()))
    }

    type ReadFileStream = GrpcFileStream;

    async fn read_file(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<Self::ReadFileStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        if let Err(error) = authorize_fs(&self.state.policy, "read", &path) {
            record_audit(&self.state, "read-stream", &path, false, &error.1);
            return Err(status_from_error(error));
        }
        let full_path = match resolve_workspace_path(&self.state.workspace, &path) {
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
        Ok(GrpcResponse::new(Box::pin(stream)))
    }

    async fn write_file(
        &self,
        request: Request<tonic::Streaming<WriteFileRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        let mut path = None;
        let mut file = None;
        let mut bytes_written = 0_u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if path.is_none() {
                if chunk.path.is_empty() {
                    return Err(Status::invalid_argument(
                        "first write chunk must include a path",
                    ));
                }
                if let Err(error) = authorize_fs(&self.state.policy, "write", &chunk.path) {
                    record_audit(&self.state, "write-stream", &chunk.path, false, &error.1);
                    return Err(status_from_error(error));
                }
                let full_path = match resolve_workspace_path(&self.state.workspace, &chunk.path) {
                    Ok(path) => path,
                    Err(error) => {
                        record_audit(&self.state, "write-stream", &chunk.path, false, &error.1);
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
                path = Some(chunk.path.clone());
            } else if !chunk.path.is_empty() && Some(&chunk.path) != path.as_ref() {
                return Err(Status::invalid_argument(
                    "write stream cannot change path after the first chunk",
                ));
            }
            if let Some(file) = &mut file {
                file.write_all(&chunk.data)
                    .await
                    .map_err(|error| Status::internal(error.to_string()))?;
                bytes_written += chunk.data.len() as u64;
            }
        }

        let Some(path) = path else {
            return Err(Status::invalid_argument(
                "write stream did not include a path",
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
    }

    async fn write_file_range(
        &self,
        request: Request<FsWriteRangeRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let write =
            grpc_fs_write_range(&self.state, request.path, request.offset, request.data).await?;
        Ok(GrpcResponse::new(write.into()))
    }

    async fn truncate_fs(
        &self,
        request: Request<FsTruncateRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let stat = grpc_fs_truncate(&self.state, request.path, request.size).await?;
        Ok(GrpcResponse::new(stat.into()))
    }

    async fn mkdir_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        let stat = grpc_fs_mkdir(&self.state, path).await?;
        Ok(GrpcResponse::new(stat.into()))
    }

    async fn delete_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsDelete>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        let path = grpc_fs_delete(&self.state, path).await?;
        Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsDelete {
            path,
        }))
    }

    async fn rename_fs(
        &self,
        request: Request<FsRenameRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsRename>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        grpc_fs_rename(&self.state, &request.from_path, &request.to_path).await?;
        Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsRename {
            from_path: request.from_path,
            to_path: request.to_path,
        }))
    }

    async fn copy_fs(
        &self,
        request: Request<FsCopyRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsCopy>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let bytes_copied = grpc_fs_copy(&self.state, &request.from_path, &request.to_path).await?;
        Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsCopy {
            from_path: request.from_path,
            to_path: request.to_path,
            bytes_copied,
        }))
    }

    async fn run_job(
        &self,
        request: Request<operon_protocol::runtime::v1::JobRunRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobRecord>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let record = start_job(
            &self.state,
            JobRunRequest {
                command: request.command,
                cwd: (!request.cwd.is_empty()).then_some(request.cwd),
                timeout_secs: request.has_timeout_secs.then_some(request.timeout_secs),
                secrets: request.secrets,
            },
        )?;
        Ok(GrpcResponse::new(record.into()))
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
        let jobs = self
            .state
            .jobs
            .lock()
            .expect("job map mutex poisoned")
            .values()
            .cloned()
            .collect();
        Ok(GrpcResponse::new(JobList { jobs }.into()))
    }

    type StreamJobLogsStream = GrpcJobLogStream;

    async fn stream_job_logs(
        &self,
        request: Request<JobIdRequest>,
    ) -> Result<GrpcResponse<Self::StreamJobLogsStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        if !self
            .state
            .jobs
            .lock()
            .expect("job map mutex poisoned")
            .contains_key(&job_id)
        {
            return Err(Status::not_found(format!("job `{job_id}` not found")));
        }
        let jobs = self.state.jobs.clone();
        let stream = async_stream::stream! {
            let mut printed = 0;
            loop {
                let snapshot = jobs
                    .lock()
                    .expect("job map mutex poisoned")
                    .get(&job_id)
                    .cloned();
                let Some(record) = snapshot else {
                    break;
                };
                for log in record.logs.iter().skip(printed) {
                    yield Ok::<_, Status>(log.clone().into());
                }
                printed = record.logs.len();
                if !matches!(record.status, JobStatus::Running) {
                    break;
                }
                time::sleep(std::time::Duration::from_millis(100)).await;
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
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if job_id.is_none() {
                if chunk.job_id.is_empty() {
                    return Err(Status::invalid_argument(
                        "first stdin chunk must include a job_id",
                    ));
                }
                sender = Some(
                    self.state
                        .job_stdin
                        .lock()
                        .expect("job stdin mutex poisoned")
                        .get(&chunk.job_id)
                        .cloned()
                        .ok_or_else(|| {
                            Status::not_found(format!("job `{}` has no open stdin", chunk.job_id))
                        })?,
                );
                job_id = Some(chunk.job_id.clone());
            } else if !chunk.job_id.is_empty() && Some(&chunk.job_id) != job_id.as_ref() {
                return Err(Status::invalid_argument(
                    "stdin stream cannot change job_id after the first chunk",
                ));
            }
            if let Some(sender) = &sender {
                bytes_written += chunk.data.len() as u64;
                sender
                    .send(chunk.data)
                    .map_err(|_| Status::failed_precondition("job stdin is closed"))?;
            }
        }
        let Some(job_id) = job_id else {
            return Err(Status::invalid_argument(
                "stdin stream did not include a job_id",
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
        let closed = self
            .state
            .job_stdin
            .lock()
            .expect("job stdin mutex poisoned")
            .remove(&job_id)
            .is_some();
        Ok(GrpcResponse::new(JobStdinClose { job_id, closed }.into()))
    }

    async fn cancel_job(
        &self,
        request: Request<operon_protocol::runtime::v1::JobCancelRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::JobRecord>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let job_id = request.into_inner().job_id;
        if let Some(sender) = self
            .state
            .job_cancel
            .lock()
            .expect("job cancel mutex poisoned")
            .remove(&job_id)
        {
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
    }

    async fn list_services(
        &self,
        request: Request<ListServicesRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(
            ServiceList {
                services: self.state.policy.service.services.clone(),
            }
            .into(),
        ))
    }

    async fn check_service(
        &self,
        request: Request<ServiceIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceCheck>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let service_id = request.into_inner().service_id;
        let check = grpc_service_check(&self.state, service_id).await?;
        Ok(GrpcResponse::new(check.into()))
    }

    async fn list_audit(
        &self,
        request: Request<ListAuditRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::AuditLog>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let events = self
            .state
            .audit
            .lock()
            .expect("audit log mutex poisoned")
            .clone();
        Ok(GrpcResponse::new(AuditLog { events }.into()))
    }
}

fn authorize_grpc(state: &AppState, metadata: &MetadataMap) -> Result<(), Status> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let Some(header) = metadata.get("authorization") else {
        return Err(Status::unauthenticated("missing bearer token"));
    };
    let Ok(header) = header.to_str() else {
        return Err(Status::unauthenticated("invalid bearer token"));
    };
    let Some(actual) = header.strip_prefix("Bearer ") else {
        return Err(Status::unauthenticated("invalid bearer token"));
    };
    if actual == expected {
        Ok(())
    } else {
        Err(Status::unauthenticated("invalid bearer token"))
    }
}

async fn grpc_fs_stat(state: &AppState, path: String) -> Result<FsStat, Status> {
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "stat", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
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
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
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
        let metadata = entry.metadata().await.map_err(status_from_io_error)?;
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
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "write-range", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
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
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "truncate", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
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
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "mkdir", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    tokio::fs::create_dir(&full_path)
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
    let full_path = match resolve_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "delete", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let metadata = tokio::fs::metadata(&full_path)
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
    let from_full_path = match resolve_workspace_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "rename", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_workspace_path(&state.workspace, to_path) {
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
    let from_full_path = match resolve_workspace_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "copy", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_workspace_path(&state.workspace, to_path) {
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
    if let Err(error) = authorize_job(&state.policy, &cwd_virtual, request.timeout_secs) {
        record_audit_capability(state, "job:default", "run", &cwd_virtual, false, &error.1);
        return Err(status_from_error(error));
    }
    let secret_env = match resolve_job_secrets(state, &request.secrets) {
        Ok(secret_env) => secret_env,
        Err(error) => {
            record_audit_capability(state, "secret:default", "use", "*", false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let cwd = match resolve_workspace_path(&state.workspace, &cwd_virtual) {
        Ok(path) => path,
        Err(error) => {
            record_audit_capability(state, "job:default", "run", &cwd_virtual, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    if !cwd.exists() {
        fs::create_dir_all(&cwd).map_err(|error| Status::internal(error.to_string()))?;
    }

    let job_id = format!("job-{}", state.next_job_id.fetch_add(1, Ordering::SeqCst));
    let record = JobRecord {
        id: job_id.clone(),
        node_id: state.node.id.clone(),
        command: request.command.clone(),
        cwd: cwd_virtual,
        status: JobStatus::Running,
        exit_code: None,
        logs: Vec::new(),
    };
    state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .insert(job_id.clone(), record.clone());
    record_audit_capability(state, "job:default", "run", &job_id, true, "allowed");
    for secret in &request.secrets {
        record_audit_capability(state, "secret:default", "use", secret, true, "allowed");
    }

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    state
        .job_cancel
        .lock()
        .expect("job cancel mutex poisoned")
        .insert(job_id.clone(), cancel_tx);
    state
        .job_stdin
        .lock()
        .expect("job stdin mutex poisoned")
        .insert(job_id.clone(), stdin_tx);

    let jobs = state.jobs.clone();
    let cancels = state.job_cancel.clone();
    let stdin = state.job_stdin.clone();
    let store = state.store.clone();
    let command = request.command;
    let timeout_secs = request
        .timeout_secs
        .unwrap_or(state.policy.job.default_timeout_secs);

    tokio::spawn(async move {
        run_job_task(JobTask {
            jobs,
            cancels,
            stdin,
            store,
            job_id,
            command,
            cwd,
            timeout_secs,
            env: secret_env,
            cancel_rx,
            stdin_rx,
        })
        .await;
    });

    Ok(record)
}

fn get_job_record(state: &AppState, job_id: &str) -> Result<JobRecord, Status> {
    state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
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

    let started = std::time::Instant::now();
    let result = time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect((service.host.as_str(), service.port)),
    )
    .await;
    let latency_ms = started.elapsed().as_millis();
    let (ok, reason) = match result {
        Ok(Ok(_)) => (true, None),
        Ok(Err(error)) => (false, Some(error.to_string())),
        Err(_) => (false, Some("service check timed out".to_string())),
    };
    record_audit_capability(
        state,
        &format!("service:{}", service.id),
        "check",
        &service.id,
        ok,
        reason.as_deref().unwrap_or("reachable"),
    );

    Ok(ServiceCheck {
        id: service.id,
        ok,
        latency_ms,
        reason,
    })
}

fn status_from_error(error: (RuntimeErrorKind, String)) -> Status {
    match error.0 {
        RuntimeErrorKind::Forbidden => Status::permission_denied(error.1),
        RuntimeErrorKind::NotFound => Status::not_found(error.1),
    }
}

fn status_from_io_error(error: std::io::Error) -> Status {
    match error.kind() {
        std::io::ErrorKind::NotFound => Status::not_found(error.to_string()),
        std::io::ErrorKind::PermissionDenied => Status::permission_denied(error.to_string()),
        std::io::ErrorKind::AlreadyExists => Status::already_exists(error.to_string()),
        std::io::ErrorKind::InvalidInput => Status::invalid_argument(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}

fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn load_policy(path: Option<&Path>) -> anyhow::Result<PolicyConfig> {
    let Some(path) = path else {
        return Ok(default_policy());
    };
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

fn load_auth_token(
    token: Option<String>,
    token_file: Option<&Path>,
) -> anyhow::Result<Option<String>> {
    match (token, token_file) {
        (Some(token), None) => Ok(Some(token)),
        (None, Some(path)) => Ok(Some(fs::read_to_string(path)?.trim().to_string())),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => anyhow::bail!("use either --auth-token or --auth-token-file"),
    }
}

fn load_secrets(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, String>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

fn load_store_jobs(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, JobRecord>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let content = fs::read_to_string(path)?;
    let mut jobs = BTreeMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)?;
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("job") {
            continue;
        }
        if let Some(record) = value.get("record") {
            let record: JobRecord = serde_json::from_value(record.clone())?;
            jobs.insert(record.id.clone(), record);
        }
    }
    Ok(jobs)
}

fn next_job_sequence(jobs: &BTreeMap<String, JobRecord>) -> u64 {
    jobs.keys()
        .filter_map(|id| id.strip_prefix("job-"))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn advertise_lan(
    node_id: &str,
    listen: SocketAddr,
    capabilities: &CapabilityList,
) -> anyhow::Result<ServiceDaemon> {
    let mdns = ServiceDaemon::new()?;
    let capability_summary = capabilities
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let endpoint = if listen.ip().is_unspecified() {
        String::new()
    } else {
        format!("grpc://{}:{}", advertised_host(listen.ip()), listen.port())
    };
    let properties = [
        ("node_id", node_id),
        ("provider", "lan"),
        ("endpoint", endpoint.as_str()),
        ("capabilities", capability_summary.as_str()),
    ];
    let service = ServiceInfo::new(
        OPERON_MDNS_SERVICE,
        node_id,
        &format!("{}.local.", node_id),
        "",
        listen.port(),
        &properties[..],
    )?
    .enable_addr_auto();
    mdns.register(service)?;
    Ok(mdns)
}

fn advertised_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) if ip == Ipv4Addr::UNSPECIFIED => "127.0.0.1".to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        ip => ip.to_string(),
    }
}

fn default_policy() -> PolicyConfig {
    PolicyConfig {
        subject: "local-cli".to_string(),
        fs: FsPolicy {
            mounts: vec![FsMountPolicy {
                name: "workspace".to_string(),
                path: "/".to_string(),
                permissions: FsPermissions {
                    read: true,
                    write: true,
                    delete: false,
                },
            }],
        },
        job: JobPolicy {
            allowed_cwds: vec!["/".to_string()],
            default_timeout_secs: 30,
            max_timeout_secs: 300,
            env_allowlist: Vec::new(),
            allowed_secrets: Vec::new(),
        },
        service: ServicePolicy::default(),
    }
}

fn default_capabilities(node_id: &str) -> CapabilityList {
    CapabilityList {
        capabilities: vec![
            Capability {
                id: "fs:workspace".to_string(),
                kind: CapabilityKind::Fs,
                node_id: node_id.to_string(),
                name: "workspace".to_string(),
                permissions: vec!["read".to_string(), "write".to_string()],
                description: "Workspace filesystem access".to_string(),
            },
            Capability {
                id: "process:default".to_string(),
                kind: CapabilityKind::Process,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["run".to_string()],
                description: "Controlled process execution".to_string(),
            },
            Capability {
                id: "job:default".to_string(),
                kind: CapabilityKind::Job,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["run".to_string(), "cancel".to_string(), "logs".to_string()],
                description: "Long-running job execution".to_string(),
            },
            Capability {
                id: "device-info:default".to_string(),
                kind: CapabilityKind::DeviceInfo,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["read".to_string()],
                description: "Node OS, architecture, and host metadata".to_string(),
            },
            Capability {
                id: "service:default".to_string(),
                kind: CapabilityKind::Service,
                node_id: node_id.to_string(),
                name: "default".to_string(),
                permissions: vec!["connect".to_string()],
                description: "Service access over an existing private network".to_string(),
            },
        ],
    }
}

fn resolve_workspace_path(
    workspace: &Path,
    virtual_path: &str,
) -> Result<PathBuf, (RuntimeErrorKind, String)> {
    let trimmed = virtual_path.trim_start_matches('/');
    let mut resolved = workspace.to_path_buf();

    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err((
                    RuntimeErrorKind::Forbidden,
                    "path escapes workspace mount".to_string(),
                ));
            }
        }
    }

    Ok(resolved)
}

fn authorize_fs(
    policy: &PolicyConfig,
    operation: &str,
    virtual_path: &str,
) -> Result<(), (RuntimeErrorKind, String)> {
    let Some(mount) = policy
        .fs
        .mounts
        .iter()
        .find(|mount| path_in_policy_scope(virtual_path, &mount.path))
    else {
        return Err((
            RuntimeErrorKind::Forbidden,
            "path is outside allowed fs mounts".to_string(),
        ));
    };

    let allowed = match operation {
        "read" => mount.permissions.read,
        "write" => mount.permissions.write,
        "delete" => mount.permissions.delete,
        _ => false,
    };

    if allowed {
        Ok(())
    } else {
        Err((
            RuntimeErrorKind::Forbidden,
            format!("fs {operation} denied by policy"),
        ))
    }
}

fn authorize_job(
    policy: &PolicyConfig,
    cwd: &str,
    requested_timeout_secs: Option<u64>,
) -> Result<(), (RuntimeErrorKind, String)> {
    if !policy
        .job
        .allowed_cwds
        .iter()
        .any(|allowed_cwd| path_in_policy_scope(cwd, allowed_cwd))
    {
        return Err((
            RuntimeErrorKind::Forbidden,
            "job cwd denied by policy".to_string(),
        ));
    }

    let timeout_secs = requested_timeout_secs.unwrap_or(policy.job.default_timeout_secs);
    if timeout_secs > policy.job.max_timeout_secs {
        return Err((
            RuntimeErrorKind::Forbidden,
            format!(
                "job timeout {timeout_secs}s exceeds policy maximum {}s",
                policy.job.max_timeout_secs
            ),
        ));
    }

    Ok(())
}

fn resolve_job_secrets(
    state: &AppState,
    requested: &[String],
) -> Result<BTreeMap<String, String>, (RuntimeErrorKind, String)> {
    let mut env = BTreeMap::new();
    for name in requested {
        if !state
            .policy
            .job
            .allowed_secrets
            .iter()
            .any(|allowed| allowed == name)
        {
            return Err((
                RuntimeErrorKind::Forbidden,
                format!("secret `{name}` denied by policy"),
            ));
        }
        let Some(value) = state.secrets.get(name) else {
            return Err((
                RuntimeErrorKind::NotFound,
                format!("secret `{name}` not found"),
            ));
        };
        env.insert(name.clone(), value.clone());
    }
    Ok(env)
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

fn path_in_policy_scope(path: &str, scope: &str) -> bool {
    let path = normalize_virtual_path(path);
    let scope = normalize_virtual_path(scope);

    scope == "/" || path == scope || path.starts_with(&format!("{scope}/"))
}

fn normalize_virtual_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    format!("/{}", trimmed.trim_matches('/'))
}

fn join_virtual_path(base: &str, name: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.is_empty() || base == "/" {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
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
    let event = AuditEvent {
        subject: state.policy.subject.clone(),
        timestamp_ms: now_ms(),
        node_id: state.node.id.clone(),
        capability: capability.to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        allowed,
        reason: reason.to_string(),
        run_id: None,
        step_id: None,
    };
    state
        .audit
        .lock()
        .expect("audit log mutex poisoned")
        .push(event.clone());
    append_store_record(
        state.store.as_deref(),
        &serde_json::json!({
            "kind": "audit",
            "event": event,
        }),
    );
}

fn append_store_record(path: Option<&Path>, record: &serde_json::Value) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            tracing::warn!(
                "failed to create store directory {}: {error}",
                parent.display()
            );
            return;
        }
    }
    let line = match serde_json::to_string(record) {
        Ok(line) => line,
        Err(error) => {
            tracing::warn!("failed to serialize store record: {error}");
            return;
        }
    };
    let mut options = fs::OpenOptions::new();
    if let Err(error) = options
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{line}")
        })
    {
        tracing::warn!("failed to append store record {}: {error}", path.display());
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn run_job_task(task: JobTask) {
    let child = TokioCommand::new("sh")
        .arg("-c")
        .arg(&task.command)
        .current_dir(task.cwd)
        .envs(task.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();

    let Ok(mut child) = child else {
        append_job_log(
            &task.jobs,
            &task.job_id,
            JobLog {
                stream: "stderr".to_string(),
                data: "failed to spawn command".to_string(),
            },
        );
        finish_job(
            &task.jobs,
            &task.cancels,
            &task.stdin,
            task.store.as_deref(),
            &task.job_id,
            JobStatus::Failed,
            None,
        );
        return;
    };

    if let Some(stdin) = child.stdin.take() {
        tokio::spawn(pump_job_stdin(task.stdin_rx, stdin));
    }
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(capture_job_stream(
            task.jobs.clone(),
            task.job_id.clone(),
            "stdout",
            stdout,
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(capture_job_stream(
            task.jobs.clone(),
            task.job_id.clone(),
            "stderr",
            stderr,
        ));
    }

    let wait = child.wait();
    tokio::pin!(wait);

    tokio::select! {
        status = &mut wait => {
            match status {
                Ok(status) => {
                    let job_status = if status.success() {
                        JobStatus::Succeeded
                    } else {
                        JobStatus::Failed
                    };
                    finish_job(
                        &task.jobs,
                        &task.cancels,
                        &task.stdin,
                        task.store.as_deref(),
                        &task.job_id,
                        job_status,
                        status.code(),
                    );
                }
                Err(error) => {
                    append_job_log(
                        &task.jobs,
                        &task.job_id,
                        JobLog {
                            stream: "stderr".to_string(),
                            data: error.to_string(),
                        },
                    );
                    finish_job(
                        &task.jobs,
                        &task.cancels,
                        &task.stdin,
                        task.store.as_deref(),
                        &task.job_id,
                        JobStatus::Failed,
                        None,
                    );
                }
            }
        }
        _ = task.cancel_rx => {
            finish_job(
                &task.jobs,
                &task.cancels,
                &task.stdin,
                task.store.as_deref(),
                &task.job_id,
                JobStatus::Cancelled,
                None,
            );
        }
        _ = time::sleep(std::time::Duration::from_secs(task.timeout_secs)) => {
            finish_job(
                &task.jobs,
                &task.cancels,
                &task.stdin,
                task.store.as_deref(),
                &task.job_id,
                JobStatus::TimedOut,
                None,
            );
        }
    }
}

async fn capture_job_stream<R>(
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
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
                &job_id,
                JobLog {
                    stream: stream.to_string(),
                    data: String::from_utf8_lossy(&buffer[..count]).to_string(),
                },
            ),
            Err(error) => {
                append_job_log(
                    &jobs,
                    &job_id,
                    JobLog {
                        stream: "stderr".to_string(),
                        data: format!("failed to read {stream}: {error}"),
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

fn append_job_log(jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>, job_id: &str, log: JobLog) {
    if let Some(record) = jobs.lock().expect("job map mutex poisoned").get_mut(job_id) {
        record.logs.push(log);
    }
}

fn finish_job(
    jobs: &Arc<Mutex<BTreeMap<String, JobRecord>>>,
    cancels: &Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    stdin: &JobStdinRegistry,
    store: Option<&Path>,
    job_id: &str,
    status: JobStatus,
    exit_code: Option<i32>,
) {
    cancels
        .lock()
        .expect("job cancel mutex poisoned")
        .remove(job_id);
    stdin
        .lock()
        .expect("job stdin mutex poisoned")
        .remove(job_id);

    if let Some(record) = jobs.lock().expect("job map mutex poisoned").get_mut(job_id) {
        record.status = status;
        record.exit_code = exit_code;
        append_store_record(
            store,
            &serde_json::json!({
                "kind": "job",
                "record": record,
            }),
        );
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

    #[test]
    fn resolves_virtual_paths_under_workspace() {
        let resolved = resolve_workspace_path(Path::new("/srv/operon"), "/nested/file.txt")
            .expect("path should resolve");

        assert_eq!(resolved, PathBuf::from("/srv/operon/nested/file.txt"));
    }

    #[test]
    fn rejects_parent_dir_workspace_escape() {
        let error = resolve_workspace_path(Path::new("/srv/operon"), "/../etc/passwd")
            .expect_err("path escape should be rejected");

        assert_eq!(error.0, RuntimeErrorKind::Forbidden);
        assert!(error.1.contains("escapes workspace"));
    }

    #[test]
    fn policy_scope_matches_exact_path_and_children_only() {
        assert!(path_in_policy_scope("/workspace", "/workspace"));
        assert!(path_in_policy_scope("/workspace/project", "/workspace"));
        assert!(!path_in_policy_scope("/workspace-other", "/workspace"));
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

        assert!(authorize_job(&policy, "/workspace/project", Some(30)).is_ok());

        let cwd_error = authorize_job(&policy, "/tmp", Some(1)).expect_err("cwd should be denied");
        assert_eq!(cwd_error.1, "job cwd denied by policy");

        let timeout_error =
            authorize_job(&policy, "/workspace", Some(31)).expect_err("timeout should be denied");
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
            audit: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        };

        let resolved = resolve_job_secrets(&state, &["TEST_SECRET".to_string()]).expect("secret");
        assert_eq!(
            resolved.get("TEST_SECRET").map(String::as_str),
            Some("secret-value")
        );

        let denied = resolve_job_secrets(&state, &["DENIED_SECRET".to_string()])
            .expect_err("denied secret should fail");
        assert_eq!(denied.0, RuntimeErrorKind::Forbidden);
    }

    #[test]
    fn audit_event_uses_policy_subject_and_capability() {
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
            audit: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            job_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            job_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        };

        record_audit_capability(&state, "fs:workspace", "read", "/file.txt", true, "allowed");

        let events = state.audit.lock().expect("audit log");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].subject, "test-subject");
        assert_eq!(events[0].node_id, "node-a");
        assert_eq!(events[0].capability, "fs:workspace");
        assert!(events[0].allowed);
    }
}
