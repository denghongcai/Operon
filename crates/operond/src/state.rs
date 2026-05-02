use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use operon_core::{
    AuditEvent, CapabilityList, JobEvent, JobLog, JobRecord, NodeInfo, PolicyConfig, RequestContext,
};
use tokio::sync::{broadcast, mpsc, oneshot};

pub(crate) const MAX_IN_MEMORY_AUDIT_EVENTS: usize = 10_000;
pub(crate) const MAX_IN_MEMORY_JOB_LOGS: usize = 10_000;
pub(crate) const MAX_IN_MEMORY_COMPLETED_JOB_LOG_BUFFERS: usize = 512;

pub(crate) type JobStdinSender = mpsc::UnboundedSender<Vec<u8>>;
pub(crate) type JobStdinRegistry = Arc<Mutex<BTreeMap<String, JobStdinSender>>>;
pub(crate) type JobEventSender = broadcast::Sender<JobEvent>;
pub(crate) type JobLogSender = broadcast::Sender<JobLog>;

#[derive(Debug, Default)]
pub(crate) struct JobLogBuffer {
    pub(crate) logs: VecDeque<JobLog>,
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
    pub(crate) jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    pub(crate) job_logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    pub(crate) job_events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    pub(crate) job_log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    pub(crate) job_cancel: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) job_stdin: JobStdinRegistry,
    pub(crate) next_job_id: Arc<AtomicU64>,
}

pub(crate) struct JobTask {
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    pub(crate) logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    pub(crate) events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    pub(crate) log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    pub(crate) cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) stdin: JobStdinRegistry,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) job_id: String,
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

pub(crate) struct JobCompletion {
    pub(crate) audit: Arc<Mutex<VecDeque<AuditEvent>>>,
    pub(crate) jobs: Arc<Mutex<BTreeMap<String, JobRecord>>>,
    pub(crate) logs: Arc<Mutex<BTreeMap<String, JobLogBuffer>>>,
    pub(crate) events: Arc<Mutex<BTreeMap<String, JobEventSender>>>,
    pub(crate) log_events: Arc<Mutex<BTreeMap<String, JobLogSender>>>,
    pub(crate) cancels: Arc<Mutex<BTreeMap<String, oneshot::Sender<()>>>>,
    pub(crate) stdin: JobStdinRegistry,
    pub(crate) store_writer: operon_store::StoreWriter,
    pub(crate) job_id: String,
    pub(crate) subject: String,
    pub(crate) node_id: String,
    pub(crate) audit_context: RequestContext,
}
