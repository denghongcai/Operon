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

pub(crate) async fn discover(
    timeout: Duration,
    output_config: Option<PathBuf>,
    check_health: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let list = operon_network::discover_lan_nodes(timeout)?;
    if let Some(path) = output_config {
        write_discovered_config(&path, &list)?;
    }
    let health = if check_health {
        check_discovered_health(&list).await
    } else {
        BTreeMap::new()
    };
    if output.json {
        if check_health {
            print_json(&discovery_view(&list, &health))?;
        } else {
            print_json(&list)?;
        }
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for row in discovery_rows(&list, &health) {
        println!("{row}");
    }
    Ok(())
}

fn write_discovered_config(path: &Path, list: &DiscoveryList) -> anyhow::Result<()> {
    let discovered = discovered_nodes(list);
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let mut config = OperonConfig::from_str_with_warnings(&content)?.config;
        for (node_id, node) in discovered {
            match config.client.nodes.get(&node_id) {
                Some(existing) if existing.endpoint == node.endpoint => {}
                Some(existing) => anyhow::bail!(
                    "discovered node `{node_id}` conflicts with existing endpoint `{}`",
                    existing.endpoint
                ),
                None => {
                    config.client.nodes.insert(node_id, node);
                }
            }
        }
        fs::write(path, serde_yaml::to_string(&config)?)?;
        return Ok(());
    }

    let nodes = discovered;

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

fn discovered_nodes(list: &DiscoveryList) -> BTreeMap<String, NodeConfig> {
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
    nodes
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiscoveryHealth {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DiscoveryView {
    nodes: Vec<DiscoveryNodeView>,
}

#[derive(Debug, serde::Serialize)]
struct DiscoveryNodeView {
    node_id: String,
    endpoint: String,
    capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<DiscoveryHealth>,
}

async fn check_discovered_health(list: &DiscoveryList) -> BTreeMap<String, DiscoveryHealth> {
    let mut health = BTreeMap::new();
    for node in &list.nodes {
        let endpoint = operon_network::NodeEndpoint {
            node_id: node.node_id.clone(),
            endpoint: node.endpoint.clone(),
            token: None,
        };
        let result = grpc::health_and_node(&endpoint).await;
        health.insert(
            node.node_id.clone(),
            match result {
                Ok((status, _)) => DiscoveryHealth {
                    ok: status.ok,
                    reason: None,
                },
                Err(error) => DiscoveryHealth {
                    ok: false,
                    reason: Some(error.to_string()),
                },
            },
        );
    }
    health
}

fn discovery_view(
    list: &DiscoveryList,
    health: &BTreeMap<String, DiscoveryHealth>,
) -> DiscoveryView {
    DiscoveryView {
        nodes: list
            .nodes
            .iter()
            .map(|node| DiscoveryNodeView {
                node_id: node.node_id.clone(),
                endpoint: node.endpoint.clone(),
                capabilities: node.capabilities.clone(),
                health: health.get(&node.node_id).cloned(),
            })
            .collect(),
    }
}

fn discovery_rows(list: &DiscoveryList, health: &BTreeMap<String, DiscoveryHealth>) -> Vec<String> {
    list.nodes
        .iter()
        .map(|node| {
            let mut row = format!("{}\t{}", node.node_id, node.endpoint);
            if let Some(health) = health.get(&node.node_id) {
                row.push_str(if health.ok {
                    "\thealth=ok"
                } else {
                    "\thealth=failed"
                });
                if let Some(reason) = &health.reason {
                    row.push_str(&format!("\treason={reason}"));
                }
            }
            row
        })
        .collect()
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
                capabilities: vec!["fs:workspace".to_string(), "exec:default".to_string()],
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

    #[test]
    fn write_discovered_config_refuses_conflicting_existing_endpoint() {
        let path = std::env::temp_dir().join(format!(
            "operon-discovery-conflict-{}-{}.yaml",
            std::process::id(),
            "endpoint"
        ));
        fs::write(
            &path,
            r#"
version: 1
client:
  nodes:
    gpu:
      endpoint: grpc://10.0.0.7:7789
"#,
        )
        .expect("write existing config");
        let list = DiscoveryList {
            nodes: vec![DiscoveryRecord {
                node_id: "gpu".to_string(),
                endpoint: "grpc://10.0.0.8:7789".to_string(),
                capabilities: Vec::new(),
            }],
        };

        let error = write_discovered_config(&path, &list).expect_err("conflict should fail");

        let _ = fs::remove_file(&path);
        assert!(error
            .to_string()
            .contains("conflicts with existing endpoint"));
    }

    #[test]
    fn discovery_rows_include_health_status_when_requested() {
        let list = DiscoveryList {
            nodes: vec![DiscoveryRecord {
                node_id: "gpu".to_string(),
                endpoint: "grpc://10.0.0.8:7789".to_string(),
                capabilities: Vec::new(),
            }],
        };
        let health = BTreeMap::from([(
            "gpu".to_string(),
            DiscoveryHealth {
                ok: false,
                reason: Some("missing bearer token".to_string()),
            },
        )]);

        let rows = discovery_rows(&list, &health);

        assert_eq!(
            rows,
            vec![
                "gpu\tgrpc://10.0.0.8:7789\thealth=failed\treason=missing bearer token".to_string()
            ]
        );
    }
}
