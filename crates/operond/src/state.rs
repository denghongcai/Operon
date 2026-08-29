use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use operon_core::{
    audit::AuditEvent,
    exec::{ExecEvent, ExecLog, ExecRecord},
    policy::PolicyConfig,
    runtime::{CapabilityList, NodeInfo, RequestContext},
};
use tokio::sync::{broadcast, mpsc, oneshot};

pub(crate) const MAX_IN_MEMORY_AUDIT_EVENTS: usize = 10_000;
pub(crate) const MAX_IN_MEMORY_EXEC_LOGS: usize = 10_000;
pub(crate) const MAX_IN_MEMORY_COMPLETED_EXEC_LOG_BUFFERS: usize = 512;

pub(crate) type ExecStdinSender = mpsc::UnboundedSender<Vec<u8>>;
pub(crate) type ExecStdinRegistry = Arc<Mutex<BTreeMap<String, ExecStdinSender>>>;
pub(crate) type ExecEventSender = broadcast::Sender<ExecEvent>;
pub(crate) type ExecLogSender = broadcast::Sender<ExecLog>;

#[derive(Debug, Default)]
pub(crate) struct ExecLogBuffer {
    pub(crate) logs: VecDeque<ExecLog>,
    pub(crate) next_sequence: u64,
    pub(crate) dropped_log_count: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecRegistry {
    pub(crate) records: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    pub(crate) logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    pub(crate) events: Arc<Mutex<BTreeMap<String, ExecEventSender>>>,
    pub(crate) log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    pub(crate) cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) stdin: ExecStdinRegistry,
    next_id: Arc<AtomicU64>,
}

impl ExecRegistry {
    pub(crate) fn new(
        records: BTreeMap<String, ExecRecord>,
        logs: BTreeMap<String, ExecLogBuffer>,
        next_id: u64,
    ) -> Self {
        Self {
            records: Arc::new(Mutex::new(records)),
            logs: Arc::new(Mutex::new(logs)),
            events: Arc::new(Mutex::new(BTreeMap::new())),
            log_events: Arc::new(Mutex::new(BTreeMap::new())),
            cancels: Arc::new(Mutex::new(BTreeMap::new())),
            stdin: Arc::new(Mutex::new(BTreeMap::new())),
            next_id: Arc::new(AtomicU64::new(next_id)),
        }
    }

    pub(crate) fn allocate_id(&self) -> String {
        format!(
            "exec-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        )
    }

    pub(crate) fn register(
        &self,
        record: ExecRecord,
        event_sender: ExecEventSender,
        log_sender: ExecLogSender,
    ) -> Result<(), tonic::Status> {
        let exec_id = record.id.clone();
        crate::locks::lock(&self.records, "exec map")?.insert(exec_id.clone(), record);
        crate::locks::lock(&self.logs, "exec log")?
            .insert(exec_id.clone(), ExecLogBuffer::default());
        crate::locks::lock(&self.events, "exec event")?.insert(exec_id.clone(), event_sender);
        crate::locks::lock(&self.log_events, "exec log event")?.insert(exec_id, log_sender);
        Ok(())
    }

    pub(crate) fn register_cancel(
        &self,
        exec_id: String,
        sender: oneshot::Sender<()>,
    ) -> Result<(), tonic::Status> {
        crate::locks::lock(&self.cancels, "exec cancel")?.insert(exec_id, sender);
        Ok(())
    }

    pub(crate) fn register_stdin(
        &self,
        exec_id: String,
        sender: ExecStdinSender,
    ) -> Result<(), tonic::Status> {
        crate::locks::lock(&self.stdin, "exec stdin")?.insert(exec_id, sender);
        Ok(())
    }
}

impl Default for ExecRegistry {
    fn default() -> Self {
        Self::new(BTreeMap::new(), BTreeMap::new(), 1)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AppState {
    pub(crate) node: NodeInfo,
    pub(crate) capabilities: CapabilityList,
    pub(crate) workspace: PathBuf,
    pub(crate) policy: PolicyConfig,
    pub(crate) auth_token: Option<String>,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) secrets: Arc<BTreeMap<String, String>>,
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) exec: ExecRegistry,
}

pub(crate) struct ExecTask {
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) execs: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    pub(crate) logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    pub(crate) events: Arc<Mutex<BTreeMap<String, ExecEventSender>>>,
    pub(crate) log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    pub(crate) cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) stdin: ExecStdinRegistry,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) exec_id: String,
    pub(crate) command: String,
    pub(crate) argv: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) timeout_secs: u64,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) subject: String,
    pub(crate) node_id: String,
    pub(crate) audit_context: RequestContext,
    pub(crate) cancel_rx: oneshot::Receiver<()>,
    pub(crate) stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

pub(crate) struct ExecCompletion {
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) execs: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    pub(crate) logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    pub(crate) events: Arc<Mutex<BTreeMap<String, ExecEventSender>>>,
    pub(crate) log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    pub(crate) cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) stdin: ExecStdinRegistry,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) exec_id: String,
    pub(crate) subject: String,
    pub(crate) node_id: String,
    pub(crate) audit_context: RequestContext,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locks::lock;
    use operon_core::ExecStatus;

    fn record(id: &str) -> ExecRecord {
        ExecRecord {
            id: id.to_string(),
            node_id: "node-a".to_string(),
            command: "echo test".to_string(),
            cwd: "/".to_string(),
            status: ExecStatus::Running,
            exit_code: None,
            log_count: 0,
            logs_truncated: false,
        }
    }

    #[test]
    fn registry_owns_id_allocation_and_initial_runtime_state() {
        let registry = ExecRegistry::default();
        let exec_id = registry.allocate_id();
        let (event_sender, _) = broadcast::channel(1);
        let (log_sender, _) = broadcast::channel(1);

        registry
            .register(record(&exec_id), event_sender, log_sender)
            .expect("register exec");

        assert_eq!(exec_id, "exec-1");
        assert!(lock(&registry.records, "exec map")
            .expect("records")
            .contains_key(&exec_id));
        assert!(lock(&registry.logs, "exec logs")
            .expect("logs")
            .contains_key(&exec_id));
        assert!(lock(&registry.events, "exec events")
            .expect("events")
            .contains_key(&exec_id));
        assert!(lock(&registry.log_events, "exec log events")
            .expect("log events")
            .contains_key(&exec_id));
    }
}
