use std::{collections::BTreeMap, fs, path::Path, path::PathBuf, time::Duration};

use operon_config::{NetworkProviderKind, NodeConfig, OperonConfig};
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
        println!(
            "{}\t{}\t{:?}",
            endpoint.node_id, endpoint.endpoint, endpoint.provider
        );
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
    println!(
        "{}\t{}\t{}",
        endpoint.node_id,
        endpoint.endpoint,
        endpoint.provider.as_str()
    );
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
    provider: &str,
    timeout: Duration,
    output_config: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    if provider != "lan" {
        anyhow::bail!("v0.3 discovery only supports --provider lan");
    }
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
        println!("{}\t{}\t{}", node.node_id, node.endpoint, node.provider);
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
                provider: NetworkProviderKind::Lan,
                auth: operon_config::AuthConfig::default(),
            },
        );
    }
    fs::write(
        path,
        serde_yaml::to_string(&OperonConfig {
            version: 1,
            daemon: None,
            client: operon_config::ClientConfig { nodes },
            policy: None,
            secrets: None,
        })?,
    )?;
    Ok(())
}
