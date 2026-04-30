#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub node_id: String,
    pub address: String,
    pub port: u16,
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
