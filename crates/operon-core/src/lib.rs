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
