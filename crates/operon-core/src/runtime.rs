pub type NodeId = String;
pub type CapabilityId = String;

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
    Job,
    DeviceInfo,
    Service,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityList {
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub next_page_token: String,
}
