use std::{collections::BTreeMap, fs, path::Path, path::PathBuf, time::Duration};

use operon_config::{ClientConfig, NodeConfig, OperonConfig};
use operon_core::{DiscoveryList, HealthStatus, NodeInfo};

use crate::{
    grpc,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

pub(crate) fn list(config_path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    let endpoints = config.endpoints(&config_dir)?;
    if output.json {
        print_json(&endpoints)?;
        return Ok(());
    }

    if output.quiet {
        return Ok(());
    }
    for endpoint in endpoints {
        println!("{}\t{}", endpoint.node_id, endpoint.endpoint);
    }

    Ok(())
}

pub(crate) fn resolve(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    let endpoint = config.endpoint(node_id, &config_dir)?;
    if output.json {
        print_json(&endpoint)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!("{}\t{}", endpoint.node_id, endpoint.endpoint);
    Ok(())
}

pub(crate) async fn ping(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;

    let (health, node): (HealthStatus, NodeInfo) = grpc::health_and_node(&endpoint).await?;
    if output.json {
        print_json(&serde_json::json!({ "health": health, "node": node }))?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!(
        "{} ok={} version={} host={} os={} arch={}",
        health.node_id, health.ok, health.version, node.hostname, node.os, node.arch
    );

    Ok(())
}

pub(crate) fn discover(
    timeout: Duration,
    output_config: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    let list = operon_network::discover_lan_nodes(timeout)?;
    if let Some(path) = output_config {
        write_discovered_config(&path, &list)?;
    }
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for node in list.nodes {
        println!("{}\t{}", node.node_id, node.endpoint);
    }
    Ok(())
}

fn write_discovered_config(path: &Path, list: &DiscoveryList) -> anyhow::Result<()> {
    let mut nodes = BTreeMap::new();
    for node in &list.nodes {
        nodes.insert(
            node.node_id.clone(),
            NodeConfig {
                endpoint: node.endpoint.clone(),
                auth: operon_config::AuthConfig::default(),
            },
        );
    }
    #[derive(serde::Serialize)]
    struct DiscoveredClientConfig {
        version: u32,
        client: ClientConfig,
    }

    fs::write(
        path,
        serde_yaml::to_string(&DiscoveredClientConfig {
            version: 1,
            client: ClientConfig { nodes },
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_core::DiscoveryRecord;

    #[test]
    fn write_discovered_config_exports_endpoint_only_nodes_without_policy() {
        let path = std::env::temp_dir().join(format!(
            "operon-v09-discovered-{}-{}.yaml",
            std::process::id(),
            "endpoint-only"
        ));
        let list = DiscoveryList {
            nodes: vec![DiscoveryRecord {
                node_id: "gpu".to_string(),
                endpoint: "grpc://10.0.0.8:7789".to_string(),
                capabilities: vec!["fs:workspace".to_string(), "job:default".to_string()],
            }],
        };

        write_discovered_config(&path, &list).expect("write discovered config");

        let yaml = fs::read_to_string(&path).expect("read discovered config");
        let _ = fs::remove_file(&path);
        assert!(yaml.contains("endpoint: grpc://10.0.0.8:7789"));
        assert!(!yaml.contains("provider:"));
        assert!(!yaml.contains("policy:"));
        assert!(!yaml.contains("daemon:"));

        let config = OperonConfig::from_str_with_warnings(&yaml).expect("parse exported config");
        assert!(config.warnings.is_empty());
        assert!(config.config.policy.is_none());
        let endpoint = config
            .config
            .endpoint("gpu", Path::new("."))
            .expect("gpu endpoint");
        assert_eq!(endpoint.endpoint, "grpc://10.0.0.8:7789");
    }
}
