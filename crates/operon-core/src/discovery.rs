use crate::NodeId;

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
