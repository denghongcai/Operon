pub type NodeId = String;
pub type CapabilityId = String;

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
    pub node_id: NodeId,
    pub capability: String,
    pub action: String,
    pub resource: String,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLog {
    pub events: Vec<AuditEvent>,
}
