use std::path::PathBuf;

use operon_config::OperonConfig;

#[derive(Debug)]
pub(crate) struct NodePath {
    pub(crate) node_id: String,
    pub(crate) path: String,
}

pub(crate) fn parse_node_path(target: &str) -> anyhow::Result<NodePath> {
    let (node_id, path) = target
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("target must be in node:/path form"))?;
    if node_id.is_empty() || path.is_empty() {
        anyhow::bail!("target must include node and path");
    }
    Ok(NodePath {
        node_id: node_id.to_string(),
        path: path.to_string(),
    })
}

pub(crate) fn load_endpoint(
    config_path: PathBuf,
    node_id: &str,
) -> anyhow::Result<operon_network::NodeEndpoint> {
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    config.endpoint(node_id, &config_dir)
}
