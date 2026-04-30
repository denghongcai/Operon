use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use operon_core::{HealthStatus, NodeInfo};
use operon_network::NodesConfig;

#[derive(Debug, Parser)]
#[command(name = "operon", about = "Operon CLI")]
struct Args {
    #[arg(short, long, default_value = "examples/nodes.yaml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    List,
    Ping { node_id: String },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => list_nodes(args.config),
            NodeCommand::Ping { node_id } => ping_node(args.config, &node_id),
        },
    }
}

fn list_nodes(config_path: PathBuf) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;

    for endpoint in config.endpoints() {
        println!(
            "{}\t{}\t{:?}",
            endpoint.node_id, endpoint.endpoint, endpoint.provider
        );
    }

    Ok(())
}

fn ping_node(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;

    let health: HealthStatus = http_get_json(&endpoint.endpoint, "/health")?;
    let node: NodeInfo = http_get_json(&endpoint.endpoint, "/node")?;

    println!(
        "{} ok={} version={} host={} os={} arch={}",
        health.node_id, health.ok, health.version, node.hostname, node.os, node.arch
    );

    Ok(())
}

fn http_get_json<T: serde::de::DeserializeOwned>(endpoint: &str, path: &str) -> anyhow::Result<T> {
    let target = parse_http_endpoint(endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
        target.host
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP response from {endpoint}"))?;
    let status = head
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status from {endpoint}"))?;

    if !status.contains(" 200 ") {
        anyhow::bail!("request to {endpoint}{path} failed: {status}");
    }

    Ok(serde_json::from_str(body)?)
}

struct HttpEndpoint {
    host: String,
    port: u16,
}

fn parse_http_endpoint(endpoint: &str) -> anyhow::Result<HttpEndpoint> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("only http:// endpoints are supported in Phase 1"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("endpoint must include a port: {endpoint}"))?;

    Ok(HttpEndpoint {
        host: host.to_string(),
        port: port.parse()?,
    })
}
