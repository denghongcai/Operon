pub type NodeId = String;
pub type CapabilityId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeErrorKind {
    Forbidden,
    NotFound,
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
    Job,
    DeviceInfo,
    Service,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityList {
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsStat {
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsList {
    pub path: String,
    pub entries: Vec<FsEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsRead {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsWriteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsWrite {
    pub path: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    pub subject: String,
    pub timestamp_ms: u128,
    pub node_id: NodeId,
    pub capability: String,
    pub action: String,
    pub resource: String,
    pub allowed: bool,
    pub reason: String,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLog {
    pub events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyConfig {
    pub subject: String,
    pub fs: FsPolicy,
    pub job: JobPolicy,
    #[serde(default)]
    pub service: ServicePolicy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsPolicy {
    pub mounts: Vec<FsMountPolicy>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsMountPolicy {
    pub name: String,
    pub path: String,
    pub permissions: FsPermissions,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsPermissions {
    pub read: bool,
    pub write: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobPolicy {
    pub allowed_cwds: Vec<String>,
    pub default_timeout_secs: u64,
    pub max_timeout_secs: u64,
    #[serde(default)]
    pub preserve_env: bool,
    pub env_allowlist: Vec<String>,
    #[serde(default)]
    pub allowed_secrets: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobRunRequest {
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ServicePolicy {
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceDefinition {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: ServiceProtocol,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceProtocol {
    Tcp,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceList {
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceCheck {
    pub id: String,
    pub ok: bool,
    pub latency_ms: u128,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobCancelRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobLog {
    pub stream: String,
    pub data: Vec<u8>,
    #[serde(default)]
    pub sequence: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub node_id: NodeId,
    pub command: String,
    pub cwd: String,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub log_count: u64,
    #[serde(default)]
    pub logs_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobLogList {
    pub job_id: String,
    pub logs: Vec<JobLog>,
    pub truncated: bool,
    pub dropped_log_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobEvent {
    pub job_id: String,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub log_count: u64,
    pub logs_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobList {
    pub jobs: Vec<JobRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobStdin {
    pub job_id: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobStdinClose {
    pub job_id: String,
    pub closed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryRecord {
    pub node_id: NodeId,
    pub endpoint: String,
    pub provider: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryList {
    pub nodes: Vec<DiscoveryRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceFile {
    pub path: String,
    pub run_id: Option<String>,
    pub name: Option<String>,
    pub status: Option<ExecutionStatus>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceFileList {
    pub traces: Vec<TraceFile>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionGraph {
    pub name: String,
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStep {
    pub id: Option<String>,
    pub node: NodeId,
    pub action: String,
    pub path: Option<String>,
    pub content: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionTrace {
    pub run_id: String,
    pub name: String,
    pub status: ExecutionStatus,
    pub steps: Vec<ExecutionStepTrace>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStepTrace {
    pub id: String,
    pub node: NodeId,
    pub action: String,
    pub status: ExecutionStatus,
    pub started_at_ms: u128,
    pub ended_at_ms: u128,
    pub error: Option<String>,
    pub output: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_uses_kebab_case_wire_names() {
        assert_eq!(
            serde_json::to_string(&JobStatus::TimedOut).expect("serialize"),
            "\"timed-out\""
        );
    }

    #[test]
    fn policy_config_round_trips_from_yaml() {
        let policy: PolicyConfig = serde_yaml::from_str(
            r#"
subject: local-cli
fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: true
        write: true
        delete: false
job:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  preserve_env: false
  env_allowlist:
    - GITHUB_TOKEN
"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.subject, "local-cli");
        assert_eq!(policy.fs.mounts[0].name, "workspace");
        assert!(policy.fs.mounts[0].permissions.read);
        assert!(!policy.fs.mounts[0].permissions.delete);
        assert_eq!(policy.job.max_timeout_secs, 300);
        assert!(!policy.job.preserve_env);
        assert_eq!(policy.job.env_allowlist, vec!["GITHUB_TOKEN"]);
        assert!(policy.service.services.is_empty());
    }

    #[test]
    fn service_policy_parses_allowed_services() {
        let policy: PolicyConfig = serde_yaml::from_str(
            r#"
subject: local-cli
fs:
  mounts: []
job:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  env_allowlist: []
service:
  services:
    - id: app
      name: app
      host: 127.0.0.1
      port: 8080
      protocol: tcp
      description: local app
"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.service.services[0].id, "app");
        assert!(matches!(
            policy.service.services[0].protocol,
            ServiceProtocol::Tcp
        ));
    }

    #[test]
    fn execution_graph_yaml_supports_mvp_step_fields() {
        let graph: ExecutionGraph = serde_yaml::from_str(
            r#"
name: copy-and-run
steps:
  - id: write-input
    node: node-a
    action: fs.write
    path: /input.txt
    content: hello
  - id: run-command
    node: node-a
    action: job.run
    cwd: /
    timeout_secs: 5
    command: cat input.txt
"#,
        )
        .expect("graph should parse");

        assert_eq!(graph.name, "copy-and-run");
        assert_eq!(graph.steps.len(), 2);
        assert_eq!(graph.steps[0].content.as_deref(), Some("hello"));
        assert_eq!(graph.steps[1].timeout_secs, Some(5));
    }
}
