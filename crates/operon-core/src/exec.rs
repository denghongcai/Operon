use crate::NodeId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecRunRequest {
    pub command: String,
    #[serde(default)]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecCancelRequest {
    pub exec_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecLog {
    pub stream: String,
    pub data: Vec<u8>,
    #[serde(default)]
    pub sequence: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecRecord {
    pub id: String,
    pub node_id: NodeId,
    pub command: String,
    pub cwd: String,
    pub status: ExecStatus,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub log_count: u64,
    #[serde(default)]
    pub logs_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecLogList {
    pub exec_id: String,
    pub logs: Vec<ExecLog>,
    pub truncated: bool,
    pub dropped_log_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecEvent {
    pub exec_id: String,
    pub status: ExecStatus,
    pub exit_code: Option<i32>,
    pub log_count: u64,
    pub logs_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecList {
    pub execs: Vec<ExecRecord>,
    #[serde(default)]
    pub next_page_token: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecStdin {
    pub exec_id: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecStdinClose {
    pub exec_id: String,
    pub closed: bool,
}
