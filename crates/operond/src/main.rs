use std::{
    collections::BTreeMap,
    convert::Infallible,
    env, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use operon_core::{
    AuditEvent, AuditLog, Capability, CapabilityKind, CapabilityList, ErrorResponse, FsEntry,
    FsList, FsMountPolicy, FsPermissions, FsPolicy, FsRead, FsStat, FsWrite, FsWriteRequest,
    HealthStatus, JobCancelRequest, JobList, JobLog, JobPolicy, JobRecord, JobRunRequest,
    JobStatus, JobStdin, JobStdinClose, NodeInfo, PolicyConfig, ServiceCheck, ServiceDefinition,
    ServiceList, ServicePolicy,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command as TokioCommand,
    sync::{mpsc, oneshot},
    time,
};
use tokio_util::io::ReaderStream;

const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

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
    #[arg(long, default_value = "127.0.0.1:7788")]
    listen: SocketAddr,

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
    job_stdin: Arc<Mutex<BTreeMap<String, mpsc::UnboundedSender<Bytes>>>>,
    next_job_id: Arc<AtomicU64>,
}

struct JobTask {
    jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    stdin: Arc<Mutex<BTreeMap<String, mpsc::UnboundedSender<Bytes>>>>,
    store: Option<PathBuf>,
    job_id: String,
    command: String,
    cwd: PathBuf,
    timeout_secs: u64,
    env: BTreeMap<String, String>,
    cancel_rx: oneshot::Receiver<()>,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    capability: Option<String>,
    resource: Option<String>,
}

impl AppError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        let code = match status {
            StatusCode::UNAUTHORIZED => "unauthorized",
            StatusCode::FORBIDDEN => "forbidden",
            StatusCode::NOT_FOUND => "not-found",
            StatusCode::BAD_REQUEST => "bad-request",
            StatusCode::REQUEST_TIMEOUT => "timeout",
            _ => "internal-error",
        };
        Self {
            status,
            code,
            message: message.into(),
            capability: None,
            resource: None,
        }
    }

    fn with_context(mut self, capability: impl Into<String>, resource: impl Into<String>) -> Self {
        self.capability = Some(capability.into());
        self.resource = Some(resource.into());
        self
    }
}

impl From<(StatusCode, String)> for AppError {
    fn from(error: (StatusCode, String)) -> Self {
        Self::new(error.0, error.1)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                code: self.code.to_string(),
                message: self.message,
                status: self.status.as_u16(),
                capability: self.capability,
                resource: self.resource,
            }),
        )
            .into_response()
    }
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
        Some(advertise_lan(&args.node_id, args.listen, &capability_list)?)
    } else {
        None
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/node", get(node_info))
        .route("/capabilities", get(capabilities))
        .route("/fs/stat", get(fs_stat))
        .route("/fs/list", get(fs_list))
        .route("/fs/read", get(fs_read))
        .route("/fs/read-stream", get(fs_read_stream))
        .route("/fs/write", post(fs_write))
        .route("/fs/write-stream", post(fs_write_stream))
        .route("/job/run", post(job_run))
        .route("/job/status", get(job_status))
        .route("/job/list", get(job_list))
        .route("/job/logs", get(job_logs))
        .route("/job/logs-stream", get(job_logs_stream))
        .route("/job/stdin", post(job_stdin))
        .route("/job/stdin/close", post(job_stdin_close))
        .route("/job/cancel", post(job_cancel))
        .route("/service/list", get(service_list))
        .route("/service/check", get(service_check))
        .route("/audit", get(audit_log))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("operond listening on {}", args.listen);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    drop(mdns);

    Ok(())
}

async fn health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<HealthStatus>, AppError> {
    authorize_request(&state, &headers)?;
    Ok(Json(HealthStatus {
        ok: true,
        node_id: state.node.id,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

async fn node_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<NodeInfo>, AppError> {
    authorize_request(&state, &headers)?;
    Ok(Json(state.node))
}

async fn capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CapabilityList>, AppError> {
    authorize_request(&state, &headers)?;
    Ok(Json(state.capabilities))
}

async fn audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuditLog>, AppError> {
    authorize_request(&state, &headers)?;
    let events = state
        .audit
        .lock()
        .expect("audit log mutex poisoned")
        .clone();
    Ok(Json(AuditLog { events }))
}

#[derive(Debug, serde::Deserialize)]
struct FsPathQuery {
    path: String,
}

#[derive(Debug, serde::Deserialize)]
struct JobIdQuery {
    id: String,
}

#[derive(Debug, serde::Deserialize)]
struct ServiceIdQuery {
    id: String,
}

async fn fs_stat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsStat>, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "read", &query.path) {
        record_audit(&state, "stat", &query.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "stat", &query.path, false, &error.1);
            return Err(error.into());
        }
    };
    let metadata = fs::metadata(&full_path).map_err(internal_error)?;
    record_audit(&state, "stat", &query.path, true, "allowed");

    Ok(Json(FsStat {
        path: query.path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
    }))
}

async fn fs_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsList>, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "read", &query.path) {
        record_audit(&state, "list", &query.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "list", &query.path, false, &error.1);
            return Err(error.into());
        }
    };
    let mut entries = Vec::new();

    for entry in fs::read_dir(&full_path).map_err(internal_error)? {
        let entry = entry.map_err(internal_error)?;
        let metadata = entry.metadata().map_err(internal_error)?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = join_virtual_path(&query.path, &name);
        entries.push(FsEntry {
            name,
            path,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    record_audit(&state, "list", &query.path, true, "allowed");

    Ok(Json(FsList {
        path: query.path,
        entries,
    }))
}

async fn fs_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsRead>, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "read", &query.path) {
        record_audit(&state, "read", &query.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "read", &query.path, false, &error.1);
            return Err(error.into());
        }
    };
    let content = fs::read_to_string(&full_path).map_err(internal_error)?;
    record_audit(&state, "read", &query.path, true, "allowed");

    Ok(Json(FsRead {
        path: query.path,
        content,
    }))
}

async fn fs_read_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FsPathQuery>,
) -> Result<Response, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "read", &query.path) {
        record_audit(&state, "read-stream", &query.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "read-stream", &query.path, false, &error.1);
            return Err(error.into());
        }
    };
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(internal_error)?;
    let file = tokio::fs::File::open(&full_path)
        .await
        .map_err(internal_error)?;
    let stream = ReaderStream::new(file);
    record_audit(&state, "read-stream", &query.path, true, "allowed");

    Ok((
        [(header::CONTENT_LENGTH, metadata.len().to_string())],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn fs_write(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FsWriteRequest>,
) -> Result<Json<FsWrite>, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "write", &request.path) {
        record_audit(&state, "write", &request.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &request.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "write", &request.path, false, &error.1);
            return Err(error.into());
        }
    };
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(internal_error)?;
    }
    fs::write(&full_path, request.content.as_bytes()).map_err(internal_error)?;
    record_audit(&state, "write", &request.path, true, "allowed");

    Ok(Json(FsWrite {
        path: request.path,
        bytes_written: request.content.len() as u64,
    }))
}

async fn fs_write_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FsPathQuery>,
    body: Body,
) -> Result<Json<FsWrite>, AppError> {
    authorize_request(&state, &headers)?;
    if let Err(error) = authorize_fs(&state.policy, "write", &query.path) {
        record_audit(&state, "write-stream", &query.path, false, &error.1);
        return Err(error.into());
    }
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "write-stream", &query.path, false, &error.1);
            return Err(error.into());
        }
    };
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(internal_error)?;
    }
    let mut file = tokio::fs::File::create(&full_path)
        .await
        .map_err(internal_error)?;
    let mut stream = body.into_data_stream();
    let mut bytes_written = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(internal_error)?;
        file.write_all(&chunk).await.map_err(internal_error)?;
        bytes_written += chunk.len() as u64;
    }
    record_audit(&state, "write-stream", &query.path, true, "allowed");

    Ok(Json(FsWrite {
        path: query.path,
        bytes_written,
    }))
}

async fn job_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JobRunRequest>,
) -> Result<Json<JobRecord>, AppError> {
    authorize_request(&state, &headers)?;
    let cwd_virtual = request.cwd.clone().unwrap_or_else(|| "/".to_string());
    if let Err(error) = authorize_job(&state.policy, &cwd_virtual, request.timeout_secs) {
        record_audit_capability(&state, "job:default", "run", &cwd_virtual, false, &error.1);
        return Err(error.into());
    }
    let secret_env = match resolve_job_secrets(&state, &request.secrets) {
        Ok(secret_env) => secret_env,
        Err(error) => {
            record_audit_capability(&state, "secret:default", "use", "*", false, &error.1);
            return Err(error.into());
        }
    };
    let cwd = match resolve_workspace_path(&state.workspace, &cwd_virtual) {
        Ok(path) => path,
        Err(error) => {
            record_audit_capability(&state, "job:default", "run", &cwd_virtual, false, &error.1);
            return Err(error.into());
        }
    };
    if !cwd.exists() {
        fs::create_dir_all(&cwd).map_err(internal_error)?;
    }

    let job_id = format!("job-{}", state.next_job_id.fetch_add(1, Ordering::SeqCst));
    let record = JobRecord {
        id: job_id.clone(),
        node_id: state.node.id.clone(),
        command: request.command.clone(),
        cwd: cwd_virtual.clone(),
        status: JobStatus::Running,
        exit_code: None,
        logs: Vec::new(),
    };
    state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .insert(job_id.clone(), record.clone());
    record_audit_capability(&state, "job:default", "run", &job_id, true, "allowed");
    for secret in &request.secrets {
        record_audit_capability(&state, "secret:default", "use", secret, true, "allowed");
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

    Ok(Json(record))
}

async fn job_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<JobIdQuery>,
) -> Result<Json<JobRecord>, AppError> {
    authorize_request(&state, &headers)?;
    let record = state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .get(&query.id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("job `{}` not found", query.id),
            )
        })?;
    Ok(Json(record))
}

async fn job_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<JobList>, AppError> {
    authorize_request(&state, &headers)?;
    let jobs = state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .values()
        .cloned()
        .collect();
    Ok(Json(JobList { jobs }))
}

async fn job_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<JobIdQuery>,
) -> Result<Json<JobRecord>, AppError> {
    authorize_request(&state, &headers)?;
    job_status(State(state), headers, Query(query)).await
}

async fn job_logs_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<JobIdQuery>,
) -> Result<Response, AppError> {
    authorize_request(&state, &headers)?;
    if !state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .contains_key(&query.id)
    {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            format!("job `{}` not found", query.id),
        )
        .with_context("job:default", query.id));
    }

    let jobs = state.jobs.clone();
    let job_id = query.id;
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
                yield Ok::<_, Infallible>(Bytes::from(log.data.clone()));
            }
            printed = record.logs.len();
            if !matches!(record.status, JobStatus::Running) {
                break;
            }
            time::sleep(std::time::Duration::from_millis(100)).await;
        }
    };

    Ok(Body::from_stream(stream).into_response())
}

async fn job_stdin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<JobIdQuery>,
    body: Body,
) -> Result<Json<JobStdin>, AppError> {
    authorize_request(&state, &headers)?;
    let sender = state
        .job_stdin
        .lock()
        .expect("job stdin mutex poisoned")
        .get(&query.id)
        .cloned()
        .ok_or_else(|| {
            AppError::new(
                StatusCode::NOT_FOUND,
                format!("job `{}` has no open stdin", query.id),
            )
            .with_context("job:default", query.id.clone())
        })?;
    let mut stream = body.into_data_stream();
    let mut bytes_written = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(internal_error)?;
        bytes_written += chunk.len() as u64;
        sender
            .send(chunk)
            .map_err(|_| (StatusCode::BAD_REQUEST, "job stdin is closed".to_string()))?;
    }
    Ok(Json(JobStdin {
        job_id: query.id,
        bytes_written,
    }))
}

async fn job_stdin_close(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<JobIdQuery>,
) -> Result<Json<JobStdinClose>, AppError> {
    authorize_request(&state, &headers)?;
    let closed = state
        .job_stdin
        .lock()
        .expect("job stdin mutex poisoned")
        .remove(&query.id)
        .is_some();
    Ok(Json(JobStdinClose {
        job_id: query.id,
        closed,
    }))
}

async fn job_cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JobCancelRequest>,
) -> Result<Json<JobRecord>, AppError> {
    authorize_request(&state, &headers)?;
    if let Some(sender) = state
        .job_cancel
        .lock()
        .expect("job cancel mutex poisoned")
        .remove(&request.job_id)
    {
        let _ = sender.send(());
        record_audit_capability(
            &state,
            "job:default",
            "cancel",
            &request.job_id,
            true,
            "cancel requested",
        );
    }

    let record = state
        .jobs
        .lock()
        .expect("job map mutex poisoned")
        .get(&request.job_id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("job `{}` not found", request.job_id),
            )
        })?;
    Ok(Json(record))
}

async fn service_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ServiceList>, AppError> {
    authorize_request(&state, &headers)?;
    Ok(Json(ServiceList {
        services: state.policy.service.services.clone(),
    }))
}

async fn service_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ServiceIdQuery>,
) -> Result<Json<ServiceCheck>, AppError> {
    authorize_request(&state, &headers)?;
    let service = match authorize_service(&state.policy, &query.id) {
        Ok(service) => service,
        Err(error) => {
            record_audit_capability(
                &state,
                "service:default",
                "check",
                &query.id,
                false,
                &error.1,
            );
            return Err(AppError::from(error).with_context("service:default", query.id));
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
        &state,
        &format!("service:{}", service.id),
        "check",
        &service.id,
        ok,
        reason.as_deref().unwrap_or("reachable"),
    );

    Ok(Json(ServiceCheck {
        id: service.id,
        ok,
        latency_ms,
        reason,
    }))
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
        format!("http://{}:{}", advertised_host(listen.ip()), listen.port())
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

fn authorize_request(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let Some(expected) = &state.auth_token else {
        return Ok(());
    };
    let Some(header) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err((StatusCode::UNAUTHORIZED, "missing bearer token".to_string()));
    };
    let Ok(header) = header.to_str() else {
        return Err((StatusCode::UNAUTHORIZED, "invalid bearer token".to_string()));
    };
    let Some(actual) = header.strip_prefix("Bearer ") else {
        return Err((StatusCode::UNAUTHORIZED, "invalid bearer token".to_string()));
    };
    if actual == expected {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "invalid bearer token".to_string()))
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
) -> Result<PathBuf, (StatusCode, String)> {
    let trimmed = virtual_path.trim_start_matches('/');
    let mut resolved = workspace.to_path_buf();

    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err((
                    StatusCode::FORBIDDEN,
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
) -> Result<(), (StatusCode, String)> {
    let Some(mount) = policy
        .fs
        .mounts
        .iter()
        .find(|mount| path_in_policy_scope(virtual_path, &mount.path))
    else {
        return Err((
            StatusCode::FORBIDDEN,
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
            StatusCode::FORBIDDEN,
            format!("fs {operation} denied by policy"),
        ))
    }
}

fn authorize_job(
    policy: &PolicyConfig,
    cwd: &str,
    requested_timeout_secs: Option<u64>,
) -> Result<(), (StatusCode, String)> {
    if !policy
        .job
        .allowed_cwds
        .iter()
        .any(|allowed_cwd| path_in_policy_scope(cwd, allowed_cwd))
    {
        return Err((
            StatusCode::FORBIDDEN,
            "job cwd denied by policy".to_string(),
        ));
    }

    let timeout_secs = requested_timeout_secs.unwrap_or(policy.job.default_timeout_secs);
    if timeout_secs > policy.job.max_timeout_secs {
        return Err((
            StatusCode::FORBIDDEN,
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
) -> Result<BTreeMap<String, String>, (StatusCode, String)> {
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
                StatusCode::FORBIDDEN,
                format!("secret `{name}` denied by policy"),
            ));
        }
        let Some(value) = state.secrets.get(name) else {
            return Err((StatusCode::NOT_FOUND, format!("secret `{name}` not found")));
        };
        env.insert(name.clone(), value.clone());
    }
    Ok(env)
}

fn authorize_service(
    policy: &PolicyConfig,
    service_id: &str,
) -> Result<ServiceDefinition, (StatusCode, String)> {
    policy
        .service
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
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

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
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
    mut receiver: mpsc::UnboundedReceiver<Bytes>,
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
    stdin: &Arc<Mutex<BTreeMap<String, mpsc::UnboundedSender<Bytes>>>>,
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
                    port: 7788,
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

        assert_eq!(error.0, StatusCode::FORBIDDEN);
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
        assert_eq!(service.port, 7788);
    }

    #[test]
    fn authorize_service_rejects_unknown_service() {
        let error =
            authorize_service(&test_policy(), "missing").expect_err("service should be denied");

        assert_eq!(error.0, StatusCode::FORBIDDEN);
        assert!(error.1.contains("denied by policy"));
    }

    #[test]
    fn fs_policy_enforces_mount_permissions() {
        let policy = test_policy();

        assert!(authorize_fs(&policy, "read", "/workspace/file.txt").is_ok());

        let write_error = authorize_fs(&policy, "write", "/workspace/file.txt")
            .expect_err("write should be denied");
        assert_eq!(write_error.0, StatusCode::FORBIDDEN);

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
        assert_eq!(denied.0, StatusCode::FORBIDDEN);
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
