use crate::NodeId;

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
