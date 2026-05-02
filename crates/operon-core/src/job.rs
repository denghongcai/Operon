use crate::NodeId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobRunRequest {
    pub command: String,
    #[serde(default)]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub secrets: Vec<String>,
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
    #[serde(default)]
    pub next_page_token: String,
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
