use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use operon_core::{
    AuditEvent, CapabilityList, ExecEvent, ExecLog, ExecRecord, NodeInfo, PolicyConfig,
    RequestContext,
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
pub(crate) struct AppState {
    pub(crate) node: NodeInfo,
    pub(crate) capabilities: CapabilityList,
    pub(crate) workspace: PathBuf,
    pub(crate) policy: PolicyConfig,
    pub(crate) auth_token: Option<String>,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) secrets: Arc<BTreeMap<String, String>>,
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) execs: Arc<Mutex<BTreeMap<String, ExecRecord>>>,
    pub(crate) exec_logs: Arc<Mutex<BTreeMap<String, ExecLogBuffer>>>,
    pub(crate) exec_events: Arc<Mutex<BTreeMap<String, ExecEventSender>>>,
    pub(crate) exec_log_events: Arc<Mutex<BTreeMap<String, ExecLogSender>>>,
    pub(crate) exec_cancel: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) exec_stdin: ExecStdinRegistry,
    pub(crate) next_exec_id: Arc<AtomicU64>,
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
