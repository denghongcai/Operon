pub type NodeId = String;
pub type CapabilityId = String;

/// Mount read scheduling limits, independent of transport deadlines.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MountReadConfig {
    /// Linux FUSE workers; None preserves the platform default.
    pub worker_threads: Option<usize>,
    pub max_inflight_reads: usize,
    /// Budget for requested bytes of in-flight RPCs, not total process RSS.
    pub max_inflight_read_mib: u32,
}

impl Default for MountReadConfig {
    fn default() -> Self {
        Self {
            worker_threads: None,
            max_inflight_reads: 8,
            max_inflight_read_mib: 32,
        }
    }
}

impl MountReadConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .worker_threads
            .is_some_and(|count| !(1..=64).contains(&count))
        {
            return Err("mount worker_threads must be between 1 and 64".into());
        }
        if !(1..=64).contains(&self.max_inflight_reads) {
            return Err("mount max_inflight_reads must be between 1 and 64".into());
        }
        if !(8..=512).contains(&self.max_inflight_read_mib) {
            return Err("mount max_inflight_read_mib must be between 8 and 512".into());
        }
        Ok(())
    }
}

/// Transport timeouts in seconds. Zero disables the corresponding timeout or
/// keepalive interval. Ping timeout must be positive when keepalive is enabled.
/// Defaults are policy, not protocol limits.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TransportConfig {
    pub connect_timeout_secs: u64,
    pub rpc_timeout_secs: u64,
    pub transfer_timeout_secs: u64,
    pub progress_timeout_secs: u64,
    pub keepalive_interval_secs: u64,
    pub keepalive_timeout_secs: u64,
    pub keepalive_while_idle: bool,
    pub adaptive_window: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10,
            rpc_timeout_secs: 30,
            transfer_timeout_secs: 600,
            progress_timeout_secs: 60,
            keepalive_interval_secs: 30,
            keepalive_timeout_secs: 10,
            keepalive_while_idle: true,
            // Opt in for links where measurements demonstrate a benefit.
            adaptive_window: false,
        }
    }
}

impl TransportConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, seconds) in [
            ("connect_timeout_secs", self.connect_timeout_secs),
            ("rpc_timeout_secs", self.rpc_timeout_secs),
            ("transfer_timeout_secs", self.transfer_timeout_secs),
            ("progress_timeout_secs", self.progress_timeout_secs),
            ("keepalive_interval_secs", self.keepalive_interval_secs),
            ("keepalive_timeout_secs", self.keepalive_timeout_secs),
        ] {
            if seconds > 604800 {
                return Err(format!(
                    "transport {name} must be between 0 and 604800 seconds"
                ));
            }
        }
        if self.keepalive_interval_secs != 0 && self.keepalive_timeout_secs == 0 {
            return Err("keepalive_timeout_secs must be positive when keepalive is enabled".into());
        }
        Ok(())
    }

    pub fn timeout(seconds: u64) -> Option<std::time::Duration> {
        (seconds != 0).then(|| std::time::Duration::from_secs(seconds))
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub node_id: String,
    pub endpoint: String,
    pub token: Option<String>,
    #[serde(default)]
    pub transport: TransportConfig,
}

impl std::fmt::Debug for NodeEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeEndpoint")
            .field("node_id", &self.node_id)
            .field("endpoint", &self.endpoint)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("transport", &self.transport)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeErrorKind {
    Forbidden,
    NotFound,
    AlreadyExists,
    InvalidArgument,
    Internal,
}

pub type RuntimeResult<T> = Result<T, (RuntimeErrorKind, String)>;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    pub run_id: Option<String>,
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeRef {
    pub id: NodeId,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityRef {
    pub node_id: NodeId,
    pub capability_id: CapabilityId,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeInfo {
    pub id: NodeId,
    pub hostname: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthStatus {
    pub ok: bool,
    pub node_id: NodeId,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub kind: CapabilityKind,
    pub node_id: NodeId,
    pub name: String,
    pub permissions: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityKind {
    Fs,
    Process,
    Exec,
    DeviceInfo,
    Service,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityList {
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub next_page_token: String,
}
