use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub node_id: String,
    pub endpoint: String,
    pub provider: NetworkProviderKind,
    pub token: Option<String>,
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

impl NetworkProviderKind {
    pub fn all() -> &'static [NetworkProviderKind] {
        &[
            NetworkProviderKind::Manual,
            NetworkProviderKind::CloudflareMesh,
            NetworkProviderKind::Tailscale,
            NetworkProviderKind::Wireguard,
            NetworkProviderKind::Ssh,
            NetworkProviderKind::Lan,
            NetworkProviderKind::Kubernetes,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkProviderKind::Manual => "manual",
            NetworkProviderKind::CloudflareMesh => "cloudflare-mesh",
            NetworkProviderKind::Tailscale => "tailscale",
            NetworkProviderKind::Wireguard => "wireguard",
            NetworkProviderKind::Ssh => "ssh",
            NetworkProviderKind::Lan => "lan",
            NetworkProviderKind::Kubernetes => "kubernetes",
        }
    }
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
    pub token: Option<String>,
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
                token: node.token.clone(),
            })
            .collect()
    }

    pub fn endpoint(&self, node_id: &str) -> Option<NodeEndpoint> {
        self.nodes.get(node_id).map(|node| NodeEndpoint {
            node_id: node_id.to_string(),
            endpoint: node.endpoint.clone(),
            provider: node.provider.clone(),
            token: node.token.clone(),
        })
    }

    pub fn resolve(&self, node_id: &str) -> anyhow::Result<NodeEndpoint> {
        self.endpoint(node_id)
            .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))
    }
}

fn default_provider() -> NetworkProviderKind {
    NetworkProviderKind::Manual
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_nodes_with_default_manual_provider() {
        let config: NodesConfig = serde_yaml::from_str(
            r#"
nodes:
  local:
    endpoint: http://127.0.0.1:7788
"#,
        )
        .expect("config should parse");

        let endpoint = config.endpoint("local").expect("local endpoint");
        assert_eq!(endpoint.node_id, "local");
        assert_eq!(endpoint.endpoint, "http://127.0.0.1:7788");
        assert!(matches!(endpoint.provider, NetworkProviderKind::Manual));
        assert_eq!(endpoint.token, None);
    }

    #[test]
    fn preserves_explicit_provider_kind() {
        let config: NodesConfig = serde_yaml::from_str(
            r#"
nodes:
  gpu:
    endpoint: http://100.96.18.20:7788
    provider: tailscale
"#,
        )
        .expect("config should parse");

        let endpoint = config.endpoint("gpu").expect("gpu endpoint");
        assert!(matches!(endpoint.provider, NetworkProviderKind::Tailscale));
    }

    #[test]
    fn returns_endpoints_in_node_id_order() {
        let config: NodesConfig = serde_yaml::from_str(
            r#"
nodes:
  node-b:
    endpoint: http://127.0.0.1:17789
  node-a:
    endpoint: http://127.0.0.1:17788
"#,
        )
        .expect("config should parse");

        let ids: Vec<_> = config
            .endpoints()
            .into_iter()
            .map(|endpoint| endpoint.node_id)
            .collect();

        assert_eq!(ids, vec!["node-a", "node-b"]);
    }

    #[test]
    fn loads_optional_node_token() {
        let config: NodesConfig = serde_yaml::from_str(
            r#"
nodes:
  local:
    endpoint: http://127.0.0.1:7788
    token: test-token
"#,
        )
        .expect("config should parse");

        let endpoint = config.endpoint("local").expect("local endpoint");
        assert_eq!(endpoint.token.as_deref(), Some("test-token"));
    }
}
