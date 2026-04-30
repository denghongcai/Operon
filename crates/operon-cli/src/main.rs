use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use operon_core::{
    AuditLog, CapabilityList, FsList, FsRead, FsStat, FsWrite, FsWriteRequest, HealthStatus,
    NodeInfo,
};
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
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    Fs {
        #[command(subcommand)]
        command: FsCommand,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    List,
    Ping { node_id: String },
}

#[derive(Debug, Subcommand)]
enum CapabilityCommand {
    List { node_id: String },
}

#[derive(Debug, Subcommand)]
enum FsCommand {
    Stat {
        target: String,
    },
    List {
        target: String,
    },
    Read {
        target: String,
    },
    Write {
        target: String,
        #[arg(long)]
        content: String,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    List { node_id: String },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => list_nodes(args.config),
            NodeCommand::Ping { node_id } => ping_node(args.config, &node_id),
        },
        Command::Capability { command } => match command {
            CapabilityCommand::List { node_id } => list_capabilities(args.config, &node_id),
        },
        Command::Fs { command } => match command {
            FsCommand::Stat { target } => fs_stat(args.config, &target),
            FsCommand::List { target } => fs_list(args.config, &target),
            FsCommand::Read { target } => fs_read(args.config, &target),
            FsCommand::Write { target, content } => fs_write(args.config, &target, &content),
        },
        Command::Audit { command } => match command {
            AuditCommand::List { node_id } => list_audit(args.config, &node_id),
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

fn list_capabilities(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;

    let list: CapabilityList = http_get_json(&endpoint.endpoint, "/capabilities")?;

    for capability in list.capabilities {
        println!(
            "{}/{}\t{:?}\t{}",
            capability.node_id,
            capability.id,
            capability.kind,
            capability.permissions.join(",")
        );
    }

    Ok(())
}

fn fs_stat(config_path: PathBuf, target: &str) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat: FsStat = http_get_json(
        &endpoint.endpoint,
        &format!("/fs/stat?path={}", encode_path(&target.path)),
    )?;

    println!(
        "{}:{} file={} dir={} size={}",
        target.node_id, stat.path, stat.is_file, stat.is_dir, stat.size
    );

    Ok(())
}

fn fs_list(config_path: PathBuf, target: &str) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let list: FsList = http_get_json(
        &endpoint.endpoint,
        &format!("/fs/list?path={}", encode_path(&target.path)),
    )?;

    for entry in list.entries {
        println!(
            "{}\t{}\t{}",
            if entry.is_dir { "dir" } else { "file" },
            entry.size,
            entry.path
        );
    }

    Ok(())
}

fn fs_read(config_path: PathBuf, target: &str) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let read: FsRead = http_get_json(
        &endpoint.endpoint,
        &format!("/fs/read?path={}", encode_path(&target.path)),
    )?;

    print!("{}", read.content);

    Ok(())
}

fn fs_write(config_path: PathBuf, target: &str, content: &str) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let request = FsWriteRequest {
        path: target.path.clone(),
        content: content.to_string(),
    };
    let write: FsWrite = http_post_json(&endpoint.endpoint, "/fs/write", &request)?;

    println!(
        "{}:{} bytes_written={}",
        target.node_id, write.path, write.bytes_written
    );

    Ok(())
}

fn list_audit(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = http_get_json(&endpoint.endpoint, "/audit")?;

    for event in audit.events {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            event.node_id,
            event.capability,
            event.action,
            event.resource,
            event.allowed,
            event.reason
        );
    }

    Ok(())
}

fn http_get_json<T: serde::de::DeserializeOwned>(endpoint: &str, path: &str) -> anyhow::Result<T> {
    http_json_request(endpoint, "GET", path, None)
}

fn http_post_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    endpoint: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<T> {
    http_json_request(endpoint, "POST", path, Some(serde_json::to_string(body)?))
}

fn http_json_request<T: serde::de::DeserializeOwned>(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<String>,
) -> anyhow::Result<T> {
    let target = parse_http_endpoint(endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let body = body.unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        target.host,
        body.len(),
        body
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

struct NodePath {
    node_id: String,
    path: String,
}

fn parse_node_path(target: &str) -> anyhow::Result<NodePath> {
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

fn load_endpoint(
    config_path: PathBuf,
    node_id: &str,
) -> anyhow::Result<operon_network::NodeEndpoint> {
    let config = NodesConfig::load(config_path)?;
    config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))
}

fn encode_path(path: &str) -> String {
    path.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
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
