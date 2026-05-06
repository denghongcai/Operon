use std::future::Future;
#[cfg(test)]
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::Path,
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use clap::Parser;
use operon_config::OperonConfig;
use operon_core::RequestContext;
#[cfg(test)]
use operon_core::{
    ExecLog, ExecPolicy, ExecRecord, ExecRunRequest, ExecStatus, FsMountPolicy, FsPermissions,
    FsPolicy, NodeInfo, PolicyConfig, RuntimeErrorKind, ServiceDefinition, ServicePolicy,
};
#[cfg(test)]
use operon_fs::authorize_fs;
#[cfg(test)]
use operon_process::{authorize_exec, resolve_exec_secrets};
use operon_protocol::runtime::v1::operon_runtime_server::OperonRuntimeServer;
use tonic::transport::Server;

mod audit;
mod auth;
mod capability_diagnostics;
mod daemon_cli;
mod daemon_state;
mod defaults;
mod exec_command;
mod exec_process;
mod exec_runtime;
mod exec_service;
mod exec_session;
mod fs_service;
mod grpc_status;
mod lan_advertise;
mod locks;
mod pagination;
mod runtime;
mod service_datagram_forward;
mod service_forward;
mod service_manager;
mod service_tcp_forward;
mod state;
mod store_config;

use audit::record_audit;
#[cfg(test)]
use audit::record_audit_capability;
use daemon_cli::{Args, Command, ServiceCommand, StartArgs};
#[cfg(test)]
use daemon_state::test_state;
#[cfg(test)]
use defaults::{capabilities_from_policy, default_policy};
use lan_advertise::advertise_lan;
use runtime::GrpcRuntime;
#[cfg(test)]
use service_forward::authorize_service;
pub(crate) use state::AppState;
#[cfg(test)]
use state::MAX_IN_MEMORY_AUDIT_EVENTS;
#[cfg(test)]
use state::{
    ExecCompletion, ExecLogBuffer, MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS,
    MAX_IN_MEMORY_EXEC_LOGS,
};

pub(crate) const MAX_FS_WRITE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_FS_FILE_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_SERVICE_DATAGRAM_BYTES: usize = 65_507;
const SERVICE_DATAGRAM_PEER_IDLE_SECS: u64 = 60;

tokio::task_local! {
    static AUDIT_CONTEXT: RequestContext;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Args::parse().command {
        Command::Start(args) => start(args).await,
        Command::Service { command } => {
            service(command)?;
            Ok(())
        }
    }
}

fn service(command: ServiceCommand) -> anyhow::Result<()> {
    match command {
        ServiceCommand::Install(args) => service_manager::install(&args.config),
        ServiceCommand::Start => service_manager::start(),
        ServiceCommand::Stop => service_manager::stop(),
        ServiceCommand::Status => service_manager::status(),
        ServiceCommand::Uninstall => service_manager::uninstall(),
        #[cfg(windows)]
        ServiceCommand::Run(args) => service_manager::run(&args.config),
        #[cfg(all(test, not(windows)))]
        ServiceCommand::Run(_) => anyhow::bail!("Windows service run is only available on Windows"),
    }
}

async fn start(args: StartArgs) -> anyhow::Result<()> {
    start_with_shutdown(args, shutdown_signal()).await
}

async fn start_with_shutdown<F>(args: StartArgs, shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()>,
{
    let config_path = args.config.unwrap_or_else(OperonConfig::default_path);
    let loaded = daemon_state::load_daemon_runtime(&config_path)?;
    let mdns = if loaded.advertise_lan {
        Some(advertise_lan(
            &loaded.node_id,
            loaded.grpc_listen,
            &loaded.capabilities,
        )?)
    } else {
        None
    };

    tracing::info!("operond gRPC listening on {}", loaded.grpc_listen);
    Server::builder()
        .add_service(OperonRuntimeServer::new(GrpcRuntime {
            state: loaded.state,
        }))
        .serve_with_shutdown(loaded.grpc_listen, shutdown)
        .await?;

    drop(mdns);

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec_runtime::{append_exec_log, finish_exec, start_exec};
    use clap::CommandFactory;
    use tokio::sync::broadcast;

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
            exec: ExecPolicy {
                allowed_cwds: vec!["/workspace".to_string()],
                default_timeout_secs: 10,
                max_timeout_secs: 30,
                allow_sessions: true,
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

    #[test]
    fn service_management_clap_exposes_commands_without_background_start() {
        let mut command = Args::command();
        let service = command
            .find_subcommand_mut("service")
            .expect("service subcommand should exist");

        for name in ["install", "start", "stop", "status", "uninstall"] {
            service
                .find_subcommand_mut(name)
                .unwrap_or_else(|| panic!("service {name} subcommand should exist"));
        }

        let start = command
            .find_subcommand_mut("start")
            .expect("start subcommand should exist");
        assert!(start
            .get_arguments()
            .all(|arg| arg.get_long() != Some("background")));
    }

    #[test]
    fn service_management_systemd_unit_runs_foreground_start_with_explicit_config() {
        let unit = service_manager::render_systemd_user_unit(
            Path::new("/opt/operon/bin/operond"),
            Path::new("/home/alice/.operon/config.yaml"),
        );

        assert!(unit.contains(
            "ExecStart=/opt/operon/bin/operond start --config /home/alice/.operon/config.yaml"
        ));
        assert!(unit.contains("Restart=on-failure"));
        assert!(!unit.contains("background"));
        assert!(!unit.contains("token:"));
    }

    #[test]
    fn service_management_launchd_plist_runs_foreground_start_with_explicit_config() {
        let plist = service_manager::render_launchd_user_plist(
            Path::new("/Applications/Operon/operond"),
            Path::new("/Users/alice/.operon/config.yaml"),
        );

        assert!(plist.contains("<key>ProgramArguments</key>"));
        assert!(plist.contains("<string>/Applications/Operon/operond</string>"));
        assert!(plist.contains("<string>start</string>"));
        assert!(plist.contains("<string>--config</string>"));
        assert!(plist.contains("<string>/Users/alice/.operon/config.yaml</string>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(!plist.contains("background"));
        assert!(!plist.contains("token:"));
    }

    #[test]
    fn service_management_windows_registration_runs_service_protocol_entrypoint() {
        let args = service_manager::windows_service_create_args(
            Path::new(r"C:\Program Files\Operon\operond.exe"),
            Path::new(r"C:\Users\alice\.operon\config.yaml"),
        );

        assert!(args.iter().any(|arg| arg == "create"));
        assert!(args.iter().any(|arg| arg == "OperonDaemon"));
        assert!(args.iter().any(|arg| arg.contains(
            r#""C:\Program Files\Operon\operond.exe" service run --config "C:\Users\alice\.operon\config.yaml""#
        )));
        assert!(!args.iter().any(|arg| arg.contains("start --config")));
    }

    #[test]
    fn service_management_windows_service_run_subcommand_is_hidden() {
        let mut command = Args::command();
        let service = command
            .find_subcommand_mut("service")
            .expect("service subcommand should exist");
        let run = service
            .find_subcommand_mut("run")
            .expect("service run subcommand should exist");

        assert!(run.is_hide_set());
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

    #[test]
    fn empty_exec_request_is_rejected_before_spawn() {
        let base = tempfile::tempdir().expect("temp dir");
        let workspace = base.path().join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        let state = test_state(test_policy(), workspace);
        let error = start_exec(
            &state,
            ExecRunRequest {
                command: String::new(),
                argv: Vec::new(),
                cwd: Some("/".to_string()),
                timeout_secs: Some(5),
                secrets: Vec::new(),
            },
        )
        .expect_err("empty exec should be invalid");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("requires command or argv"));
    }

    #[tokio::test]
    async fn read_range_reads_only_requested_bytes() {
        let base = tempfile::tempdir().expect("temp dir");
        let workspace = base.path().join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(workspace.join("data.bin"), b"0123456789").expect("file");
        let state = test_state(default_policy(), workspace);

        let chunk = fs_service::read_range(&state, "/data.bin".to_string(), 3, 4)
            .await
            .expect("read range");

        assert_eq!(chunk.data, b"3456");
    }

    #[tokio::test]
    async fn list_fs_returns_deterministic_pages() {
        let base = tempfile::tempdir().expect("temp dir");
        let workspace = base.path().join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(workspace.join("b.txt"), "b").expect("b");
        fs::write(workspace.join("a.txt"), "a").expect("a");
        fs::write(workspace.join("c.txt"), "c").expect("c");
        let state = test_state(default_policy(), workspace);

        let first = fs_service::list_page(&state, "/".to_string(), 2, "")
            .await
            .expect("first page");
        assert_eq!(
            first
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );
        assert_eq!(first.next_page_token, "2");

        let second = fs_service::list_page(&state, "/".to_string(), 2, &first.next_page_token)
            .await
            .expect("second page");
        assert_eq!(
            second
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["c.txt"]
        );
        assert!(second.next_page_token.is_empty());

        let invalid = fs_service::list_page(&state, "/".to_string(), 2, "not-a-number")
            .await
            .expect_err("invalid token");
        assert_eq!(invalid.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn mkdir_creates_missing_parent_directories() {
        let base = tempfile::tempdir().expect("temp dir");
        let workspace = base.path().join("workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        let state = test_state(default_policy(), workspace.clone());

        fs_service::mkdir(&state, "/a/b/c".to_string())
            .await
            .expect("mkdir nested");

        assert!(workspace.join("a/b/c").is_dir());
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
        let base = tempfile::tempdir().expect("temp dir");
        let workspace = base.path().join("workspace");
        let outside = base.path().join("outside");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&outside).expect("outside");
        let target = outside.join("secret.txt");
        fs::write(&target, "secret").expect("target");
        let link = workspace.join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let mut policy = default_policy();
        policy.fs.mounts[0].permissions.delete = true;
        let state = test_state(policy, workspace);

        fs_service::delete(&state, "/link".to_string(), None)
            .await
            .expect("delete symlink");

        assert!(!link.exists());
        assert_eq!(
            fs::read_to_string(target).expect("target still exists"),
            "secret"
        );
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
    fn exec_policy_enforces_cwd_and_timeout() {
        let policy = test_policy();

        assert!(authorize_exec(&policy.exec, "/workspace/project", Some(30)).is_ok());

        let cwd_error =
            authorize_exec(&policy.exec, "/tmp", Some(1)).expect_err("cwd should be denied");
        assert_eq!(cwd_error.1, "exec cwd denied by policy");

        let timeout_error = authorize_exec(&policy.exec, "/workspace", Some(31))
            .expect_err("timeout should be denied");
        assert!(timeout_error.1.contains("exceeds policy maximum"));
    }

    #[test]
    fn denied_exec_policy_audit_uses_reason_code() {
        let state = test_state(test_policy(), PathBuf::from("/workspace"));

        let error = start_exec(
            &state,
            ExecRunRequest {
                command: "pwd".to_string(),
                argv: Vec::new(),
                cwd: Some("/tmp".to_string()),
                timeout_secs: Some(1),
                secrets: Vec::new(),
            },
        )
        .expect_err("exec cwd should be denied");

        assert_eq!(error.code(), tonic::Code::PermissionDenied);
        let audit = state.audit.lock().expect("audit");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].capability, "exec:default");
        assert_eq!(audit[0].action, "run");
        assert_eq!(audit[0].resource, "/tmp");
        assert!(!audit[0].allowed);
        assert_eq!(
            audit[0].reason,
            "exec-cwd-denied: exec cwd denied by policy"
        );
    }

    #[test]
    fn exec_secrets_must_be_allowed_and_defined() {
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
            execs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_logs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            exec_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_exec_id: Arc::new(AtomicU64::new(1)),
        };

        let resolved = resolve_exec_secrets(
            &state.policy.exec,
            &state.secrets,
            &["TEST_SECRET".to_string()],
        )
        .expect("secret");
        assert_eq!(
            resolved.get("TEST_SECRET").map(String::as_str),
            Some("secret-value")
        );

        let denied = resolve_exec_secrets(
            &state.policy.exec,
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
            execs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_logs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            exec_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_exec_id: Arc::new(AtomicU64::new(1)),
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
            execs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_logs: Arc::new(Mutex::new(BTreeMap::new())),
            exec_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_log_events: Arc::new(Mutex::new(BTreeMap::new())),
            exec_cancel: Arc::new(Mutex::new(BTreeMap::new())),
            exec_stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_exec_id: Arc::new(AtomicU64::new(1)),
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
    fn exec_logs_are_separate_and_bounded() {
        let execs = Arc::new(Mutex::new(BTreeMap::from([(
            "exec-1".to_string(),
            ExecRecord {
                id: "exec-1".to_string(),
                node_id: "node-a".to_string(),
                command: "echo test".to_string(),
                cwd: "/workspace".to_string(),
                status: ExecStatus::Running,
                exit_code: None,
                log_count: 0,
                logs_truncated: false,
            },
        )])));
        let logs = Arc::new(Mutex::new(BTreeMap::from([(
            "exec-1".to_string(),
            ExecLogBuffer::default(),
        )])));
        let (sender, _) = broadcast::channel(1);
        let log_events = Arc::new(Mutex::new(BTreeMap::from([("exec-1".to_string(), sender)])));

        for index in 0..(MAX_IN_MEMORY_EXEC_LOGS + 5) {
            append_exec_log(
                &execs,
                &logs,
                &log_events,
                &operon_store::StoreWriter::new(None),
                "exec-1",
                ExecLog {
                    stream: "stdout".to_string(),
                    data: format!("line {index}").into_bytes(),
                    sequence: 0,
                },
            );
        }

        let record = execs
            .lock()
            .expect("exec map")
            .get("exec-1")
            .expect("exec")
            .clone();
        assert_eq!(record.log_count, (MAX_IN_MEMORY_EXEC_LOGS + 5) as u64);
        assert!(record.logs_truncated);

        let buffers = logs.lock().expect("exec logs");
        let buffer = buffers.get("exec-1").expect("exec log buffer");
        assert_eq!(buffer.logs.len(), MAX_IN_MEMORY_EXEC_LOGS);
        assert_eq!(buffer.logs.front().expect("first retained").sequence, 5);
    }

    #[test]
    fn finished_exec_runtime_state_is_cleaned_and_log_buffers_are_bounded() {
        let mut exec_records = BTreeMap::new();
        let mut log_buffers = BTreeMap::new();
        for index in 1..=(MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS + 2) {
            let exec_id = format!("exec-{index}");
            exec_records.insert(
                exec_id.clone(),
                ExecRecord {
                    id: exec_id.clone(),
                    node_id: "node-a".to_string(),
                    command: "true".to_string(),
                    cwd: "/workspace".to_string(),
                    status: ExecStatus::Succeeded,
                    exit_code: Some(0),
                    log_count: 0,
                    logs_truncated: false,
                },
            );
            log_buffers.insert(exec_id, ExecLogBuffer::default());
        }
        let target_exec_id = format!("exec-{}", MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS + 2);
        if let Some(record) = exec_records.get_mut(&target_exec_id) {
            record.status = ExecStatus::Running;
            record.exit_code = None;
        }

        let execs = Arc::new(Mutex::new(exec_records));
        let logs = Arc::new(Mutex::new(log_buffers));
        let (event_sender, _) = broadcast::channel(1);
        let (log_sender, _) = broadcast::channel(1);
        let events = Arc::new(Mutex::new(BTreeMap::from([(
            target_exec_id.clone(),
            event_sender,
        )])));
        let log_events = Arc::new(Mutex::new(BTreeMap::from([(
            target_exec_id.clone(),
            log_sender,
        )])));
        let completion = ExecCompletion {
            audit: Arc::new(Mutex::new(VecDeque::new())),
            execs,
            logs: logs.clone(),
            events: events.clone(),
            log_events: log_events.clone(),
            cancels: Arc::new(Mutex::new(BTreeMap::new())),
            stdin: Arc::new(Mutex::new(BTreeMap::new())),
            store_writer: operon_store::StoreWriter::new(None),
            exec_id: target_exec_id.clone(),
            subject: "test-subject".to_string(),
            node_id: "node-a".to_string(),
            audit_context: RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
        };

        finish_exec(&completion, ExecStatus::Succeeded, Some(0));

        assert!(!events.lock().expect("events").contains_key(&target_exec_id));
        assert!(!log_events
            .lock()
            .expect("log events")
            .contains_key(&target_exec_id));
        let logs = logs.lock().expect("logs");
        assert_eq!(logs.len(), MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS);
        assert!(!logs.contains_key("exec-1"));
        assert!(!logs.contains_key("exec-2"));
        assert!(logs.contains_key(&target_exec_id));
    }

    #[test]
    fn finish_exec_records_terminal_audit_event() {
        let exec_id = "exec-1".to_string();
        let execs = Arc::new(Mutex::new(BTreeMap::from([(
            exec_id.clone(),
            ExecRecord {
                id: exec_id.clone(),
                node_id: "node-a".to_string(),
                command: "true".to_string(),
                cwd: "/".to_string(),
                status: ExecStatus::Running,
                exit_code: None,
                log_count: 0,
                logs_truncated: false,
            },
        )])));
        let logs = Arc::new(Mutex::new(BTreeMap::new()));
        let (event_sender, _) = broadcast::channel(1);
        let (log_sender, _) = broadcast::channel(1);
        let audit = Arc::new(Mutex::new(VecDeque::new()));
        let completion = ExecCompletion {
            audit: audit.clone(),
            execs,
            logs,
            events: Arc::new(Mutex::new(BTreeMap::from([(
                exec_id.clone(),
                event_sender,
            )]))),
            log_events: Arc::new(Mutex::new(BTreeMap::from([(exec_id.clone(), log_sender)]))),
            cancels: Arc::new(Mutex::new(BTreeMap::new())),
            stdin: Arc::new(Mutex::new(BTreeMap::new())),
            store_writer: operon_store::StoreWriter::new(None),
            exec_id: exec_id.clone(),
            subject: "test-subject".to_string(),
            node_id: "node-a".to_string(),
            audit_context: RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
        };

        finish_exec(&completion, ExecStatus::Failed, Some(7));

        let events = audit.lock().expect("audit");
        let event = events.back().expect("completion audit");
        assert_eq!(event.capability, "exec:default");
        assert_eq!(event.action, "finish");
        assert_eq!(event.resource, exec_id);
        assert!(event.allowed);
        assert_eq!(event.reason, "status=failed exit_code=7");
        assert_eq!(event.run_id.as_deref(), Some("run-1"));
        assert_eq!(event.step_id.as_deref(), Some("step-1"));
    }
}
