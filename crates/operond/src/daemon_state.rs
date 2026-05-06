use std::{
    collections::BTreeMap,
    env, fs,
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

pub(crate) fn load_daemon_runtime(config_path: &Path) -> anyhow::Result<LoadedDaemonRuntime> {
    let config = OperonConfig::load(config_path)?;
    let config_dir = OperonConfig::config_dir(config_path);
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
    let stored_execs = operon_store::load_execs(store.as_deref())?;
    let stored_audit_events = operon_store::load_audit_events(store.as_deref())?;
    let stored_exec_logs = operon_store::load_exec_logs(store.as_deref())?;
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
}
