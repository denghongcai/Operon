use crate::NodeId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    pub subject: String,
    pub timestamp_ms: u64,
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
    #[serde(default)]
    pub next_page_token: String,
}
