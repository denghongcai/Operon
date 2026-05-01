use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use operon_core::{
    AuditLog, CapabilityList, ErrorResponse, FsList, FsRead, FsStat, FsWrite, FsWriteRequest,
    HealthStatus, JobCancelRequest, JobRecord, JobRunRequest, JobStatus, NodeInfo,
};
use operon_network::{NetworkProviderKind, NodeEndpoint, NodesConfig};

mod graph;

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
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
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
    Job {
        #[command(subcommand)]
        command: JobCommand,
    },
    Run {
        workflow: PathBuf,
        #[arg(long)]
        trace_output: Option<PathBuf>,
    },
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    List,
    Resolve { node_id: String },
    Ping { node_id: String },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    List,
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
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Write {
        target: String,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    List { node_id: String },
}

#[derive(Debug, Subcommand)]
enum JobCommand {
    Run {
        node_id: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
        #[arg(long)]
        secret: Vec<String>,
        #[arg(long)]
        detach: bool,
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    Status {
        node_id: String,
        job_id: String,
    },
    Logs {
        node_id: String,
        job_id: String,
        #[arg(long)]
        follow: bool,
    },
    Cancel {
        node_id: String,
        job_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    Show { path: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => list_nodes(args.config),
            NodeCommand::Resolve { node_id } => resolve_node(args.config, &node_id),
            NodeCommand::Ping { node_id } => ping_node(args.config, &node_id),
        },
        Command::Provider { command } => match command {
            ProviderCommand::List => list_providers(),
        },
        Command::Capability { command } => match command {
            CapabilityCommand::List { node_id } => list_capabilities(args.config, &node_id),
        },
        Command::Fs { command } => match command {
            FsCommand::Stat { target } => fs_stat(args.config, &target),
            FsCommand::List { target } => fs_list(args.config, &target),
            FsCommand::Read { target, output } => fs_read(args.config, &target, output),
            FsCommand::Write {
                target,
                content,
                file,
            } => fs_write(args.config, &target, content, file),
        },
        Command::Audit { command } => match command {
            AuditCommand::List { node_id } => list_audit(args.config, &node_id),
        },
        Command::Job { command } => match command {
            JobCommand::Run {
                node_id,
                cwd,
                timeout_secs,
                secret,
                detach,
                command,
            } => job_run(
                args.config,
                &node_id,
                cwd,
                timeout_secs,
                secret,
                detach,
                command,
            ),
            JobCommand::Status { node_id, job_id } => job_status(args.config, &node_id, &job_id),
            JobCommand::Logs {
                node_id,
                job_id,
                follow,
            } => job_logs(args.config, &node_id, &job_id, follow),
            JobCommand::Cancel { node_id, job_id } => job_cancel(args.config, &node_id, &job_id),
        },
        Command::Run {
            workflow,
            trace_output,
        } => graph::run_graph(args.config, workflow, trace_output),
        Command::Trace { command } => match command {
            TraceCommand::Show { path } => trace_show(path),
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

fn list_providers() -> anyhow::Result<()> {
    for provider in NetworkProviderKind::all() {
        println!("{}", provider.as_str());
    }
    Ok(())
}

fn resolve_node(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config.resolve(node_id)?;
    println!(
        "{}\t{}\t{}",
        endpoint.node_id,
        endpoint.endpoint,
        endpoint.provider.as_str()
    );
    Ok(())
}

fn ping_node(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;

    let health: HealthStatus = http_get_json_endpoint(&endpoint, "/health")?;
    let node: NodeInfo = http_get_json_endpoint(&endpoint, "/node")?;

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

    let list: CapabilityList = http_get_json_endpoint(&endpoint, "/capabilities")?;

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
    let stat: FsStat = http_get_json_endpoint(
        &endpoint,
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
    let list: FsList = http_get_json_endpoint(
        &endpoint,
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

fn fs_read(config_path: PathBuf, target: &str, output: Option<PathBuf>) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;

    if let Some(output) = output {
        let content = http_get_bytes_endpoint(
            &endpoint,
            &format!("/fs/read-stream?path={}", encode_path(&target.path)),
        )?;
        std::fs::write(output, content)?;
    } else {
        let read: FsRead = http_get_json_endpoint(
            &endpoint,
            &format!("/fs/read?path={}", encode_path(&target.path)),
        )?;
        print!("{}", read.content);
    }

    Ok(())
}

fn fs_write(
    config_path: PathBuf,
    target: &str,
    content: Option<String>,
    file: Option<PathBuf>,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;

    let write: FsWrite = match (content, file) {
        (Some(content), None) => {
            let request = FsWriteRequest {
                path: target.path.clone(),
                content,
            };
            http_post_json_endpoint(&endpoint, "/fs/write", &request)?
        }
        (None, Some(file)) => {
            let body = std::fs::read(file)?;
            http_post_bytes_endpoint(
                &endpoint,
                &format!("/fs/write-stream?path={}", encode_path(&target.path)),
                &body,
            )?
        }
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("fs write requires --content or --file"),
    };

    println!(
        "{}:{} bytes_written={}",
        target.node_id, write.path, write.bytes_written
    );

    Ok(())
}

fn list_audit(config_path: PathBuf, node_id: &str) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = http_get_json_endpoint(&endpoint, "/audit")?;

    for event in audit.events {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            event.subject,
            event.timestamp_ms,
            event.node_id,
            event.capability,
            event.action,
            event.resource,
            event.allowed,
            event.reason,
            event.run_id.as_deref().unwrap_or("-"),
            event.step_id.as_deref().unwrap_or("-")
        );
    }

    Ok(())
}

fn job_run(
    config_path: PathBuf,
    node_id: &str,
    cwd: Option<String>,
    timeout_secs: u64,
    secrets: Vec<String>,
    detach: bool,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path.clone(), node_id)?;
    let request = JobRunRequest {
        command: command.join(" "),
        cwd,
        timeout_secs: Some(timeout_secs),
        secrets,
    };
    let record: JobRecord = http_post_json_endpoint(&endpoint, "/job/run", &request)?;
    println!(
        "{} {} {:?} {}",
        record.node_id, record.id, record.status, record.command
    );

    if !detach {
        wait_for_job(config_path, node_id, &record.id)?;
    }

    Ok(())
}

fn trace_show(path: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn job_status(config_path: PathBuf, node_id: &str, job_id: &str) -> anyhow::Result<()> {
    let record = load_job(config_path, node_id, job_id)?;
    print_job_status(&record);
    Ok(())
}

fn job_logs(config_path: PathBuf, node_id: &str, job_id: &str, follow: bool) -> anyhow::Result<()> {
    if !follow {
        let record = load_job_from_path(config_path, node_id, &format!("/job/logs?id={job_id}"))?;
        for log in record.logs {
            print!("{}", log.data);
        }
        return Ok(());
    }

    let mut printed = 0;
    loop {
        let record = load_job_from_path(
            config_path.clone(),
            node_id,
            &format!("/job/logs?id={job_id}"),
        )?;
        for log in record.logs.iter().skip(printed) {
            print!("{}", log.data);
        }
        std::io::stdout().flush()?;
        printed = record.logs.len();
        if !matches!(record.status, JobStatus::Running) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

fn job_cancel(config_path: PathBuf, node_id: &str, job_id: &str) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let request = JobCancelRequest {
        job_id: job_id.to_string(),
    };
    let record: JobRecord = http_post_json_endpoint(&endpoint, "/job/cancel", &request)?;
    print_job_status(&record);
    Ok(())
}

fn wait_for_job(config_path: PathBuf, node_id: &str, job_id: &str) -> anyhow::Result<()> {
    loop {
        let record = load_job(config_path.clone(), node_id, job_id)?;
        match record.status {
            JobStatus::Running => std::thread::sleep(std::time::Duration::from_millis(100)),
            _ => {
                print_job_status(&record);
                for log in record.logs {
                    print!("{}", log.data);
                }
                return Ok(());
            }
        }
    }
}

pub(crate) fn load_job(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
) -> anyhow::Result<JobRecord> {
    load_job_from_path(config_path, node_id, &format!("/job/status?id={job_id}"))
}

fn load_job_from_path(
    config_path: PathBuf,
    node_id: &str,
    path: &str,
) -> anyhow::Result<JobRecord> {
    let endpoint = load_endpoint(config_path, node_id)?;
    http_get_json_endpoint(&endpoint, path)
}

fn print_job_status(record: &JobRecord) {
    println!(
        "{} {} {:?} exit_code={:?}",
        record.node_id, record.id, record.status, record.exit_code
    );
}

pub(crate) fn http_get_json_endpoint<T: serde::de::DeserializeOwned>(
    endpoint: &NodeEndpoint,
    path: &str,
) -> anyhow::Result<T> {
    http_json_request(
        &endpoint.endpoint,
        "GET",
        path,
        None,
        endpoint.token.as_deref(),
    )
}

fn http_get_bytes_endpoint(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<Vec<u8>> {
    http_bytes_request(
        &endpoint.endpoint,
        "GET",
        path,
        None,
        "application/octet-stream",
        endpoint.token.as_deref(),
    )
}

pub(crate) fn http_post_json_endpoint<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    endpoint: &NodeEndpoint,
    path: &str,
    body: &B,
) -> anyhow::Result<T> {
    http_json_request(
        &endpoint.endpoint,
        "POST",
        path,
        Some(serde_json::to_string(body)?),
        endpoint.token.as_deref(),
    )
}

fn http_post_bytes_endpoint<T: serde::de::DeserializeOwned>(
    endpoint: &NodeEndpoint,
    path: &str,
    body: &[u8],
) -> anyhow::Result<T> {
    let response = http_bytes_request(
        &endpoint.endpoint,
        "POST",
        path,
        Some(body),
        "application/octet-stream",
        endpoint.token.as_deref(),
    )?;
    Ok(serde_json::from_slice(&response)?)
}

fn http_json_request<T: serde::de::DeserializeOwned>(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<String>,
    token: Option<&str>,
) -> anyhow::Result<T> {
    let target = parse_http_endpoint(endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let body = body.unwrap_or_default();
    let auth = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
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
        anyhow::bail!(
            "request to {endpoint}{path} failed: {status}: {}",
            format_error_body(body)
        );
    }

    Ok(serde_json::from_str(body)?)
}

fn http_bytes_request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    content_type: &str,
    token: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let target = parse_http_endpoint(endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let body = body.unwrap_or_default();
    let auth = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: */*\r\n{auth}Content-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
        target.host,
        body.len(),
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP response from {endpoint}"))?;
    let head = std::str::from_utf8(&response[..split])?;
    let body = response[split + 4..].to_vec();
    let status = head
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status from {endpoint}"))?;

    if !status.contains(" 200 ") {
        anyhow::bail!(
            "request to {endpoint}{path} failed: {status}: {}",
            format_error_body(&String::from_utf8_lossy(&body))
        );
    }

    Ok(body)
}

fn format_error_body(body: &str) -> String {
    serde_json::from_str::<ErrorResponse>(body)
        .map(|error| format!("{}: {}", error.code, error.message))
        .unwrap_or_else(|_| body.trim().to_string())
}

#[derive(Debug)]
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

pub(crate) fn load_endpoint(
    config_path: PathBuf,
    node_id: &str,
) -> anyhow::Result<operon_network::NodeEndpoint> {
    let config = NodesConfig::load(config_path)?;
    config.resolve(node_id)
}

pub(crate) fn encode_path(path: &str) -> String {
    path.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}

#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_node_path_target() {
        let target = parse_node_path("node-a:/workspace/file.txt").expect("target should parse");

        assert_eq!(target.node_id, "node-a");
        assert_eq!(target.path, "/workspace/file.txt");
    }

    #[test]
    fn rejects_node_path_without_separator() {
        let error = parse_node_path("node-a/workspace").expect_err("target should fail");
        assert!(error.to_string().contains("node:/path"));
    }

    #[test]
    fn encodes_query_path_reserved_characters() {
        assert_eq!(
            encode_path("/a b/%file?x=1&y=2#frag"),
            "/a%20b/%25file%3Fx=1%26y=2%23frag"
        );
    }

    #[test]
    fn parses_http_endpoint_with_port() {
        let endpoint =
            parse_http_endpoint("http://127.0.0.1:17788").expect("endpoint should parse");

        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 17788);
    }

    #[test]
    fn rejects_non_http_endpoint_for_current_client() {
        let error = parse_http_endpoint("https://127.0.0.1:7788").expect_err("https unsupported");
        assert!(error.to_string().contains("only http://"));
    }

    #[test]
    fn formats_structured_daemon_error_body() {
        let body = r#"{"code":"forbidden","message":"fs read denied by policy"}"#;
        assert_eq!(
            format_error_body(body),
            "forbidden: fs read denied by policy"
        );
    }
}
