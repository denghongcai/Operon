use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub node_id: String,
    pub endpoint: String,
    pub provider: NetworkProviderKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkProviderKind {
    Manual,
    CloudflareMesh,
    Tailscale,
    Wireguard,
    Ssh,
    Lan,
    Kubernetes,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodesConfig {
    pub nodes: BTreeMap<String, NodeConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeConfig {
    pub endpoint: String,
    #[serde(default = "default_provider")]
    pub provider: NetworkProviderKind,
}

impl NodesConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn endpoints(&self) -> Vec<NodeEndpoint> {
        self.nodes
            .iter()
            .map(|(node_id, node)| NodeEndpoint {
                node_id: node_id.clone(),
                endpoint: node.endpoint.clone(),
                provider: node.provider.clone(),
            })
            .collect()
    }

    pub fn endpoint(&self, node_id: &str) -> Option<NodeEndpoint> {
        self.nodes.get(node_id).map(|node| NodeEndpoint {
            node_id: node_id.to_string(),
            endpoint: node.endpoint.clone(),
            provider: node.provider.clone(),
        })
    }
}

fn default_provider() -> NetworkProviderKind {
    NetworkProviderKind::Manual
}
