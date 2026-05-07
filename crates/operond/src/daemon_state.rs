use std::{
    collections::BTreeMap,
    env, fmt, fs,
    net::SocketAddr,
    path::Path,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use operon_config::{resolve_path, OperonConfig};
use operon_core::{CapabilityList, NodeInfo};

use crate::{
    audit::bounded_audit_events,
    defaults::{capabilities_from_policy, default_policy},
    exec_runtime::{exec_log_buffers_from_persisted_logs, next_exec_sequence},
    state::AppState,
    store_config::resolve_store_path,
};

pub(crate) struct LoadedDaemonRuntime {
    pub(crate) state: AppState,
    pub(crate) grpc_listen: SocketAddr,
    pub(crate) node_id: String,
    pub(crate) advertise_lan: bool,
    pub(crate) capabilities: CapabilityList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DaemonStartupErrorKind {
    ConfigLoad,
    ConfigParse,
    DaemonSection,
    AuthToken,
    StoreConfig,
    StateRestore,
    Secrets,
    ServerBind,
}

impl DaemonStartupErrorKind {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ConfigLoad => "config-load",
            Self::ConfigParse => "config-parse",
            Self::DaemonSection => "daemon-section",
            Self::AuthToken => "auth-token",
            Self::StoreConfig => "store-config",
            Self::StateRestore => "state-restore",
            Self::Secrets => "secrets",
            Self::ServerBind => "server-bind",
        }
    }
}

#[derive(Debug)]
pub(crate) struct DaemonStartupError {
    kind: DaemonStartupErrorKind,
    message: String,
    source: Option<anyhow::Error>,
}

impl DaemonStartupError {
    fn new(kind: DaemonStartupErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    fn with_source(
        kind: DaemonStartupErrorKind,
        message: impl Into<String>,
        source: impl Into<anyhow::Error>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            source: Some(source.into()),
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for DaemonStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "daemon startup error [{}]: {}",
            self.code(),
            self.message
        )?;
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for DaemonStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source.as_ref())
    }
}

pub(crate) fn load_daemon_runtime(config_path: &Path) -> anyhow::Result<LoadedDaemonRuntime> {
    let config = load_config(config_path)?;
    let config_dir = OperonConfig::config_dir(config_path);
    let daemon = config.daemon.clone().ok_or_else(|| {
        DaemonStartupError::new(
            DaemonStartupErrorKind::DaemonSection,
            "config is missing daemon section",
        )
    })?;
    let policy = config.policy.unwrap_or_else(default_policy);
    let auth_token = daemon.auth.resolve(&config_dir).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::AuthToken,
            "failed to resolve daemon auth token",
            error,
        )
    })?;
    let store = resolve_store_path(&config_dir, daemon.store.as_deref()).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::StoreConfig,
            "failed to resolve daemon store path",
            error,
        )
    })?;
    let store_writer = operon_store::StoreWriter::new(store.clone());
    let secrets_path = config
        .secrets
        .as_ref()
        .and_then(|secrets| secrets.file.as_ref())
        .map(|path| resolve_path(&config_dir, path));
    let secrets = load_secrets(secrets_path.as_deref())?;
    let stored_execs = operon_store::load_execs(store.as_deref()).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::StateRestore,
            "failed to restore persisted exec records",
            error,
        )
    })?;
    let stored_audit_events =
        operon_store::load_audit_events(store.as_deref()).map_err(|error| {
            DaemonStartupError::with_source(
                DaemonStartupErrorKind::StateRestore,
                "failed to restore persisted audit events",
                error,
            )
        })?;
    let stored_exec_logs = operon_store::load_exec_logs(store.as_deref()).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::StateRestore,
            "failed to restore persisted exec logs",
            error,
        )
    })?;
    let next_exec_id = next_exec_sequence(&stored_execs);
    let node = NodeInfo {
        id: daemon.node_id.clone(),
        hostname: hostname(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    };
    let capabilities = capabilities_from_policy(&node.id, &policy);
    let execs = Arc::new(Mutex::new(stored_execs));
    let state = AppState {
        capabilities: capabilities.clone(),
        node,
        workspace: daemon.workspace,
        policy,
        auth_token,
        store_writer,
        secrets: Arc::new(secrets),
        audit: Arc::new(Mutex::new(bounded_audit_events(stored_audit_events))),
        execs,
        exec_logs: Arc::new(Mutex::new(exec_log_buffers_from_persisted_logs(
            stored_exec_logs,
        ))),
        exec_events: Arc::new(Mutex::new(BTreeMap::new())),
        exec_log_events: Arc::new(Mutex::new(BTreeMap::new())),
        exec_cancel: Arc::new(Mutex::new(BTreeMap::new())),
        exec_stdin: Arc::new(Mutex::new(BTreeMap::new())),
        next_exec_id: Arc::new(AtomicU64::new(next_exec_id)),
    };

    Ok(LoadedDaemonRuntime {
        state,
        grpc_listen: daemon.grpc_listen,
        node_id: daemon.node_id,
        advertise_lan: daemon.advertise_lan,
        capabilities,
    })
}

fn load_config(config_path: &Path) -> anyhow::Result<OperonConfig> {
    let content = fs::read_to_string(config_path).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::ConfigLoad,
            format!("failed to read config {}", config_path.display()),
            error,
        )
    })?;
    let loaded = OperonConfig::from_str_with_warnings(&content).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::ConfigParse,
            format!("failed to parse config {}", config_path.display()),
            error,
        )
    })?;
    for warning in &loaded.warnings {
        eprintln!("warning: unknown config field `{}` ignored", warning.path);
    }
    Ok(loaded.config)
}

pub(crate) fn server_start_error(
    listen: SocketAddr,
    source: impl Into<anyhow::Error>,
) -> anyhow::Error {
    DaemonStartupError::with_source(
        DaemonStartupErrorKind::ServerBind,
        format!("failed to start gRPC server on {listen}"),
        source,
    )
    .into()
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
    let content = fs::read_to_string(path).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::Secrets,
            format!("failed to read secrets file {}", path.display()),
            error,
        )
    })?;
    serde_yaml::from_str(&content).map_err(|error| {
        DaemonStartupError::with_source(
            DaemonStartupErrorKind::Secrets,
            format!("failed to parse secrets file {}", path.display()),
            error,
        )
        .into()
    })
}

#[cfg(test)]
pub(crate) fn test_state(
    policy: operon_core::PolicyConfig,
    workspace: std::path::PathBuf,
) -> AppState {
    AppState {
        node: NodeInfo {
            id: "node-a".to_string(),
            hostname: "host".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        },
        capabilities: capabilities_from_policy("node-a", &policy),
        workspace,
        policy,
        auth_token: None,
        store_writer: operon_store::StoreWriter::new(None),
        secrets: Arc::new(BTreeMap::new()),
        audit: Arc::new(Mutex::new(std::collections::VecDeque::new())),
        execs: Arc::new(Mutex::new(BTreeMap::new())),
        exec_logs: Arc::new(Mutex::new(BTreeMap::new())),
        exec_events: Arc::new(Mutex::new(BTreeMap::new())),
        exec_log_events: Arc::new(Mutex::new(BTreeMap::new())),
        exec_cancel: Arc::new(Mutex::new(BTreeMap::new())),
        exec_stdin: Arc::new(Mutex::new(BTreeMap::new())),
        next_exec_id: Arc::new(AtomicU64::new(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_derives_capabilities_from_supplied_policy() {
        let mut policy = default_policy();
        policy.exec.allow_sessions = false;

        let state = test_state(policy, std::path::PathBuf::from("/workspace"));

        let exec = state
            .capabilities
            .capabilities
            .iter()
            .find(|capability| capability.id == "exec:default")
            .expect("exec capability");
        assert!(!exec
            .permissions
            .iter()
            .any(|permission| permission == "session"));
    }

    #[test]
    fn daemon_state_startup_errors_classify_missing_config_file() {
        let base = tempfile::tempdir().expect("temp dir");
        let error = load_runtime_error(&base.path().join("missing.yaml"), "missing config");

        assert_startup_error(&error, DaemonStartupErrorKind::ConfigLoad);
    }

    #[test]
    fn daemon_state_startup_errors_classify_invalid_config_yaml() {
        let base = tempfile::tempdir().expect("temp dir");
        let config = base.path().join("config.yaml");
        fs::write(&config, "version: [").expect("write config");

        let error = load_runtime_error(&config, "invalid config");

        assert_startup_error(&error, DaemonStartupErrorKind::ConfigParse);
    }

    #[test]
    fn daemon_state_startup_errors_classify_missing_daemon_section() {
        let base = tempfile::tempdir().expect("temp dir");
        let config = base.path().join("config.yaml");
        fs::write(&config, "version: 1\n").expect("write config");

        let error = load_runtime_error(&config, "missing daemon section");

        assert_startup_error(&error, DaemonStartupErrorKind::DaemonSection);
    }

    #[test]
    fn daemon_state_startup_errors_classify_auth_token_resolution() {
        let base = tempfile::tempdir().expect("temp dir");
        let config = base.path().join("config.yaml");
        fs::write(
            &config,
            r#"
version: 1
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:0
  workspace: /workspace
  auth:
    token_file: missing-token
"#,
        )
        .expect("write config");

        let error = load_runtime_error(&config, "missing token");

        assert_startup_error(&error, DaemonStartupErrorKind::AuthToken);
    }

    #[test]
    fn daemon_state_startup_errors_classify_store_config() {
        let base = tempfile::tempdir().expect("temp dir");
        let config_dir = base.path().join("config");
        let outside = base.path().join("outside");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&outside).expect("outside dir");
        let config = config_dir.join("config.yaml");
        fs::write(
            &config,
            format!(
                r#"
version: 1
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:0
  workspace: /workspace
  store: {}
  auth:
    token: test-token
"#,
                outside.join("store.jsonl").display()
            ),
        )
        .expect("write config");

        let error = load_runtime_error(&config, "outside store");

        assert_startup_error(&error, DaemonStartupErrorKind::StoreConfig);
    }

    #[test]
    fn daemon_state_startup_errors_classify_state_restore() {
        let base = tempfile::tempdir().expect("temp dir");
        let config = base.path().join("config.yaml");
        fs::write(base.path().join("store.jsonl"), "not-json\n").expect("write store");
        fs::write(
            &config,
            r#"
version: 1
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:0
  workspace: /workspace
  store: store.jsonl
  auth:
    token: test-token
"#,
        )
        .expect("write config");

        let error = load_runtime_error(&config, "invalid store");

        assert_startup_error(&error, DaemonStartupErrorKind::StateRestore);
    }

    #[test]
    fn daemon_state_startup_errors_classify_secrets() {
        let base = tempfile::tempdir().expect("temp dir");
        let config = base.path().join("config.yaml");
        fs::write(base.path().join("secrets.yaml"), "token: [").expect("write secrets");
        fs::write(
            &config,
            r#"
version: 1
secrets:
  file: secrets.yaml
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:0
  workspace: /workspace
  auth:
    token: test-token
"#,
        )
        .expect("write config");

        let error = load_runtime_error(&config, "invalid secrets");

        assert_startup_error(&error, DaemonStartupErrorKind::Secrets);
    }

    #[test]
    fn daemon_state_startup_errors_classify_server_bind() {
        let listen: SocketAddr = "127.0.0.1:7789".parse().expect("listen address");
        let error = server_start_error(
            listen,
            std::io::Error::new(std::io::ErrorKind::AddrInUse, "address already in use"),
        );

        let startup = assert_startup_error(&error, DaemonStartupErrorKind::ServerBind);
        assert!(startup.to_string().contains("127.0.0.1:7789"));
    }

    fn load_runtime_error(path: &Path, scenario: &str) -> anyhow::Error {
        match load_daemon_runtime(path) {
            Ok(_) => panic!("{scenario} should fail"),
            Err(error) => error,
        }
    }

    fn assert_startup_error(
        error: &anyhow::Error,
        kind: DaemonStartupErrorKind,
    ) -> &DaemonStartupError {
        let startup = error
            .downcast_ref::<DaemonStartupError>()
            .expect("daemon startup error");
        assert_eq!(startup.kind, kind);
        assert!(startup.to_string().contains(kind.code()));
        startup
    }
}
