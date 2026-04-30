use std::{
    env, fs,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use operon_core::{
    AuditEvent, AuditLog, Capability, CapabilityKind, CapabilityList, FsEntry, FsList, FsRead,
    FsStat, FsWrite, FsWriteRequest, HealthStatus, NodeInfo,
};

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
}

#[derive(Debug, Clone)]
struct AppState {
    node: NodeInfo,
    capabilities: CapabilityList,
    workspace: PathBuf,
    audit: Arc<Mutex<Vec<AuditEvent>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Args::parse().command {
        Command::Start(args) => start(args).await,
    }
}

async fn start(args: StartArgs) -> anyhow::Result<()> {
    let node = NodeInfo {
        id: args.node_id,
        hostname: hostname(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    };
    let state = AppState {
        capabilities: default_capabilities(&node.id),
        node,
        workspace: args.workspace,
        audit: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/node", get(node_info))
        .route("/capabilities", get(capabilities))
        .route("/fs/stat", get(fs_stat))
        .route("/fs/list", get(fs_list))
        .route("/fs/read", get(fs_read))
        .route("/fs/write", post(fs_write))
        .route("/audit", get(audit_log))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("operond listening on {}", args.listen);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthStatus> {
    Json(HealthStatus {
        ok: true,
        node_id: state.node.id,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn node_info(State(state): State<AppState>) -> Json<NodeInfo> {
    Json(state.node)
}

async fn capabilities(State(state): State<AppState>) -> Json<CapabilityList> {
    Json(state.capabilities)
}

async fn audit_log(State(state): State<AppState>) -> Json<AuditLog> {
    let events = state
        .audit
        .lock()
        .expect("audit log mutex poisoned")
        .clone();
    Json(AuditLog { events })
}

#[derive(Debug, serde::Deserialize)]
struct FsPathQuery {
    path: String,
}

async fn fs_stat(
    State(state): State<AppState>,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsStat>, (StatusCode, String)> {
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "stat", &query.path, false, &error.1);
            return Err(error);
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
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsList>, (StatusCode, String)> {
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "list", &query.path, false, &error.1);
            return Err(error);
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
    Query(query): Query<FsPathQuery>,
) -> Result<Json<FsRead>, (StatusCode, String)> {
    let full_path = match resolve_workspace_path(&state.workspace, &query.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "read", &query.path, false, &error.1);
            return Err(error);
        }
    };
    let content = fs::read_to_string(&full_path).map_err(internal_error)?;
    record_audit(&state, "read", &query.path, true, "allowed");

    Ok(Json(FsRead {
        path: query.path,
        content,
    }))
}

async fn fs_write(
    State(state): State<AppState>,
    Json(request): Json<FsWriteRequest>,
) -> Result<Json<FsWrite>, (StatusCode, String)> {
    let full_path = match resolve_workspace_path(&state.workspace, &request.path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(&state, "write", &request.path, false, &error.1);
            return Err(error);
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

fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
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
    let event = AuditEvent {
        node_id: state.node.id.clone(),
        capability: "fs:workspace".to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        allowed,
        reason: reason.to_string(),
    };
    state
        .audit
        .lock()
        .expect("audit log mutex poisoned")
        .push(event);
}
