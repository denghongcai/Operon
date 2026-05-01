use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use operon_core::{
    AuditLog, CapabilityList, DiscoveryList, DiscoveryRecord, ErrorResponse, ExecutionTrace,
    FsList, FsRead, FsStat, FsWrite, FsWriteRequest, HealthStatus, JobCancelRequest, JobList,
    JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose, NodeInfo, ServiceCheck,
    ServiceList, ServiceProtocol, TraceFile, TraceFileList,
};
use operon_network::{NetworkProviderKind, NodeEndpoint, NodesConfig};

mod graph;

const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

#[derive(Debug, Parser)]
#[command(name = "operon", about = "Operon CLI")]
struct Args {
    #[arg(short, long, default_value = "examples/nodes.yaml")]
    config: PathBuf,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    Init {
        #[command(subcommand)]
        command: InitCommand,
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
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
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
    Mount {
        #[command(subcommand)]
        command: MountCommand,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    List,
    Discover {
        #[arg(long, default_value = "lan")]
        provider: String,
        #[arg(long, default_value_t = 3)]
        timeout_secs: u64,
        #[arg(long)]
        output_config: Option<PathBuf>,
    },
    Resolve {
        node_id: String,
    },
    Ping {
        node_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum InitCommand {
    Config { path: PathBuf },
    Policy { path: PathBuf },
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
    List {
        node_id: String,
    },
    Show {
        node_id: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        action: Option<String>,
        #[arg(long)]
        allowed: Option<bool>,
        #[arg(long)]
        resource: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    List { node_id: String },
    Check { node_id: String, service_id: String },
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
    List {
        node_id: String,
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
        #[arg(long)]
        stream: bool,
    },
    Stdin {
        node_id: String,
        job_id: String,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        close: bool,
    },
    Cancel {
        node_id: String,
        job_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    Show {
        path: PathBuf,
    },
    List {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum MountCommand {
    ReadOnly {
        target: String,
        #[arg(long)]
        to: PathBuf,
    },
}

#[derive(Debug, Clone, Copy)]
struct OutputMode {
    json: bool,
    quiet: bool,
}

struct JobRunInput {
    config_path: PathBuf,
    node_id: String,
    cwd: Option<String>,
    timeout_secs: u64,
    secrets: Vec<String>,
    detach: bool,
    command: Vec<String>,
    output: OutputMode,
}

#[derive(Debug, Default)]
struct AuditFilter {
    limit: Option<usize>,
    capability: Option<String>,
    action: Option<String>,
    allowed: Option<bool>,
    resource: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let output = OutputMode {
        json: args.json,
        quiet: args.quiet,
    };

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => list_nodes(args.config, output),
            NodeCommand::Discover {
                provider,
                timeout_secs,
                output_config,
            } => discover_nodes(
                &provider,
                Duration::from_secs(timeout_secs),
                output_config,
                output,
            ),
            NodeCommand::Resolve { node_id } => resolve_node(args.config, &node_id, output),
            NodeCommand::Ping { node_id } => ping_node(args.config, &node_id, output),
        },
        Command::Init { command } => match command {
            InitCommand::Config { path } => init_config(path, output),
            InitCommand::Policy { path } => init_policy(path, output),
        },
        Command::Provider { command } => match command {
            ProviderCommand::List => list_providers(output),
        },
        Command::Capability { command } => match command {
            CapabilityCommand::List { node_id } => list_capabilities(args.config, &node_id, output),
        },
        Command::Fs { command } => match command {
            FsCommand::Stat { target } => fs_stat(args.config, &target, output),
            FsCommand::List { target } => fs_list(args.config, &target, output),
            FsCommand::Read {
                target,
                output: file_output,
            } => fs_read(args.config, &target, file_output, output),
            FsCommand::Write {
                target,
                content,
                file,
            } => fs_write(args.config, &target, content, file, output),
        },
        Command::Audit { command } => match command {
            AuditCommand::List { node_id } => list_audit(args.config, &node_id, output),
            AuditCommand::Show {
                node_id,
                limit,
                capability,
                action,
                allowed,
                resource,
            } => {
                let filter = AuditFilter {
                    limit,
                    capability,
                    action,
                    allowed,
                    resource,
                };
                audit_show(args.config, &node_id, filter, output)
            }
        },
        Command::Service { command } => match command {
            ServiceCommand::List { node_id } => service_list(args.config, &node_id, output),
            ServiceCommand::Check {
                node_id,
                service_id,
            } => service_check(args.config, &node_id, &service_id, output),
        },
        Command::Job { command } => match command {
            JobCommand::Run {
                node_id,
                cwd,
                timeout_secs,
                secret,
                detach,
                command,
            } => job_run(JobRunInput {
                config_path: args.config,
                node_id,
                cwd,
                timeout_secs,
                secrets: secret,
                detach,
                command,
                output,
            }),
            JobCommand::List { node_id } => job_list(args.config, &node_id, output),
            JobCommand::Status { node_id, job_id } => {
                job_status(args.config, &node_id, &job_id, output)
            }
            JobCommand::Logs {
                node_id,
                job_id,
                follow,
                stream,
            } => job_logs(args.config, &node_id, &job_id, follow, stream),
            JobCommand::Stdin {
                node_id,
                job_id,
                content,
                file,
                close,
            } => job_stdin(args.config, &node_id, &job_id, content, file, close, output),
            JobCommand::Cancel { node_id, job_id } => {
                job_cancel(args.config, &node_id, &job_id, output)
            }
        },
        Command::Run {
            workflow,
            trace_output,
        } => graph::run_graph(args.config, workflow, trace_output),
        Command::Trace { command } => match command {
            TraceCommand::Show { path } => trace_show(path, output),
            TraceCommand::List { dir } => trace_list(dir, output),
        },
        Command::Mount { command } => match command {
            MountCommand::ReadOnly { target, to } => {
                mount_read_only(args.config, &target, to, output)
            }
        },
    }
}

fn list_nodes(config_path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoints = config.endpoints();
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

fn list_providers(output: OutputMode) -> anyhow::Result<()> {
    let providers: Vec<_> = NetworkProviderKind::all()
        .iter()
        .map(NetworkProviderKind::as_str)
        .collect();
    if output.json {
        print_json(&providers)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for provider in providers {
        println!("{provider}");
    }
    Ok(())
}

fn resolve_node(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config.resolve(node_id)?;
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

fn ping_node(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;

    let health: HealthStatus = http_get_json_endpoint(&endpoint, "/health")?;
    let node: NodeInfo = http_get_json_endpoint(&endpoint, "/node")?;
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

fn list_capabilities(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let config = NodesConfig::load(config_path)?;
    let endpoint = config
        .endpoint(node_id)
        .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;

    let list: CapabilityList = http_get_json_endpoint(&endpoint, "/capabilities")?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

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

fn fs_stat(config_path: PathBuf, target: &str, output: OutputMode) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat: FsStat = http_get_json_endpoint(
        &endpoint,
        &format!("/fs/stat?path={}", encode_path(&target.path)),
    )?;
    if output.json {
        print_json(&stat)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!(
        "{}:{} file={} dir={} size={}",
        target.node_id, stat.path, stat.is_file, stat.is_dir, stat.size
    );

    Ok(())
}

fn fs_list(config_path: PathBuf, target: &str, output: OutputMode) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let list: FsList = http_get_json_endpoint(
        &endpoint,
        &format!("/fs/list?path={}", encode_path(&target.path)),
    )?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

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

fn fs_read(
    config_path: PathBuf,
    target: &str,
    file_output: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;

    if let Some(file_output) = file_output {
        http_get_bytes_to_file_endpoint(
            &endpoint,
            &format!("/fs/read-stream?path={}", encode_path(&target.path)),
            &file_output,
        )?;
    } else {
        let read: FsRead = http_get_json_endpoint(
            &endpoint,
            &format!("/fs/read?path={}", encode_path(&target.path)),
        )?;
        if output.json {
            print_json(&read)?;
            return Ok(());
        }
        if output.quiet {
            return Ok(());
        }
        print!("{}", read.content);
    }

    Ok(())
}

fn fs_write(
    config_path: PathBuf,
    target: &str,
    content: Option<String>,
    file: Option<PathBuf>,
    output: OutputMode,
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
        (None, Some(file)) => http_post_file_endpoint(
            &endpoint,
            &format!("/fs/write-stream?path={}", encode_path(&target.path)),
            &file,
        )?,
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("fs write requires --content or --file"),
    };
    if output.json {
        print_json(&write)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    println!(
        "{}:{} bytes_written={}",
        target.node_id, write.path, write.bytes_written
    );

    Ok(())
}

fn list_audit(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = http_get_json_endpoint(&endpoint, "/audit")?;
    if output.json {
        print_json(&audit)?;
        return Ok(());
    }
    print_audit(audit, AuditFilter::default(), output)
}

fn audit_show(
    config_path: PathBuf,
    node_id: &str,
    filter: AuditFilter,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = http_get_json_endpoint(&endpoint, "/audit")?;
    if output.json {
        print_json(&audit)?;
        return Ok(());
    }
    print_audit(audit, filter, output)
}

fn print_audit(audit: AuditLog, filter: AuditFilter, output: OutputMode) -> anyhow::Result<()> {
    if output.quiet {
        return Ok(());
    }
    let mut events = audit
        .events
        .into_iter()
        .filter(|event| {
            filter
                .capability
                .as_ref()
                .map_or(true, |capability| &event.capability == capability)
                && filter
                    .action
                    .as_ref()
                    .map_or(true, |action| &event.action == action)
                && filter
                    .allowed
                    .map_or(true, |allowed| event.allowed == allowed)
                && filter
                    .resource
                    .as_ref()
                    .map_or(true, |resource| event.resource.contains(resource))
        })
        .collect::<Vec<_>>();
    if let Some(limit) = filter.limit {
        events = events.into_iter().rev().take(limit).collect::<Vec<_>>();
    }
    for event in events {
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

fn service_list(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let list: ServiceList = http_get_json_endpoint(&endpoint, "/service/list")?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for service in list.services {
        println!(
            "{}\t{}\t{}:{}\t{}",
            service.id,
            service.name,
            service.host,
            service.port,
            format_service_protocol(&service.protocol)
        );
    }
    Ok(())
}

fn service_check(
    config_path: PathBuf,
    node_id: &str,
    service_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let check: ServiceCheck = http_get_json_endpoint(
        &endpoint,
        &format!("/service/check?id={}", encode_path(service_id)),
    )?;
    if output.json {
        print_json(&check)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{} ok={} latency_ms={} reason={}",
        check.id,
        check.ok,
        check.latency_ms,
        check.reason.as_deref().unwrap_or("-")
    );
    Ok(())
}

fn job_run(input: JobRunInput) -> anyhow::Result<()> {
    let endpoint = load_endpoint(input.config_path.clone(), &input.node_id)?;
    let request = JobRunRequest {
        command: input.command.join(" "),
        cwd: input.cwd,
        timeout_secs: Some(input.timeout_secs),
        secrets: input.secrets,
    };
    let record: JobRecord = http_post_json_endpoint(&endpoint, "/job/run", &request)?;
    if input.output.json {
        print_json(&record)?;
    } else if !input.output.quiet {
        println!(
            "{} {} {:?} {}",
            record.node_id, record.id, record.status, record.command
        );
    }

    if !input.detach {
        wait_for_job(input.config_path, &input.node_id, &record.id, input.output)?;
    }

    Ok(())
}

fn trace_show(path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)?;
    if output.quiet {
        return Ok(());
    }
    if output.json {
        let value: serde_json::Value = serde_json::from_str(&content)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let trace: ExecutionTrace = serde_json::from_str(&content)?;
    println!("{} {} {:?}", trace.run_id, trace.name, trace.status);
    for step in trace.steps {
        println!(
            "{}\t{}\t{}\t{:?}\t{}ms\t{}",
            step.id,
            step.node,
            step.action,
            step.status,
            step.ended_at_ms.saturating_sub(step.started_at_ms),
            step.error.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn job_list(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let list: JobList = http_get_json_endpoint(&endpoint, "/job/list")?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for record in list.jobs {
        print_job_status(&record);
    }
    Ok(())
}

fn job_status(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let record = load_job(config_path, node_id, job_id)?;
    if output.json {
        print_json(&record)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_job_status(&record);
    Ok(())
}

fn job_logs(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    follow: bool,
    stream: bool,
) -> anyhow::Result<()> {
    if stream {
        let endpoint = load_endpoint(config_path, node_id)?;
        return http_get_bytes_to_writer_endpoint(
            &endpoint,
            &format!("/job/logs-stream?id={job_id}"),
            &mut std::io::stdout(),
        );
    }
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

fn job_stdin(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    content: Option<String>,
    file: Option<PathBuf>,
    close: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    if close {
        let closed: JobStdinClose = http_post_json_endpoint(
            &endpoint,
            &format!("/job/stdin/close?id={job_id}"),
            &serde_json::json!({}),
        )?;
        if output.json {
            print_json(&closed)?;
        } else if !output.quiet {
            println!("{} stdin_closed={}", closed.job_id, closed.closed);
        }
        return Ok(());
    }
    let written: JobStdin = match (content, file) {
        (Some(content), None) => http_post_bytes_endpoint(
            &endpoint,
            &format!("/job/stdin?id={job_id}"),
            content.as_bytes(),
        )?,
        (None, Some(file)) => {
            http_post_file_endpoint(&endpoint, &format!("/job/stdin?id={job_id}"), &file)?
        }
        (Some(_), Some(_)) => anyhow::bail!("use either --content or --file, not both"),
        (None, None) => anyhow::bail!("job stdin requires --content, --file, or --close"),
    };
    if output.json {
        print_json(&written)?;
    } else if !output.quiet {
        println!(
            "{} stdin_bytes_written={}",
            written.job_id, written.bytes_written
        );
    }
    Ok(())
}

fn job_cancel(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let request = JobCancelRequest {
        job_id: job_id.to_string(),
    };
    let record: JobRecord = http_post_json_endpoint(&endpoint, "/job/cancel", &request)?;
    if output.json {
        print_json(&record)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_job_status(&record);
    Ok(())
}

fn wait_for_job(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    loop {
        let record = load_job(config_path.clone(), node_id, job_id)?;
        match record.status {
            JobStatus::Running => std::thread::sleep(std::time::Duration::from_millis(100)),
            _ => {
                if output.json {
                    print_json(&record)?;
                } else if !output.quiet {
                    print_job_status(&record);
                    for log in record.logs {
                        print!("{}", log.data);
                    }
                }
                return Ok(());
            }
        }
    }
}

fn trace_list(dir: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let mut traces = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let Ok(trace) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if !(trace.get("run_id").is_some()
            && trace.get("name").is_some()
            && trace.get("steps").is_some())
        {
            continue;
        }
        traces.push(TraceFile {
            path: path.display().to_string(),
            run_id: trace
                .get("run_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            name: trace
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            status: trace
                .get("status")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok()),
        });
    }
    traces.sort_by(|a, b| a.path.cmp(&b.path));
    let list = TraceFileList { traces };
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for trace in list.traces {
        println!(
            "{}\t{}\t{}",
            trace.path,
            trace.run_id.as_deref().unwrap_or("-"),
            trace.name.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn init_config(path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let content = r#"nodes:
  local:
    endpoint: http://127.0.0.1:7788
    provider: manual
    token: change-me
"#;
    fs::write(&path, content)?;
    if !output.quiet {
        println!("{}", path.display());
    }
    Ok(())
}

fn init_policy(path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let content = r#"subject: local-cli

fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: true
        write: true
        delete: false

job:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  env_allowlist: []
  allowed_secrets: []

service:
  services:
    - id: local-daemon
      name: local-daemon
      host: 127.0.0.1
      port: 7788
      protocol: tcp
      description: Operon daemon TCP health surface
"#;
    fs::write(&path, content)?;
    if !output.quiet {
        println!("{}", path.display());
    }
    Ok(())
}

fn discover_nodes(
    provider: &str,
    timeout: Duration,
    output_config: Option<PathBuf>,
    output: OutputMode,
) -> anyhow::Result<()> {
    if provider != "lan" {
        anyhow::bail!("v0.3 discovery only supports --provider lan");
    }
    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse(OPERON_MDNS_SERVICE)?;
    let deadline = Instant::now() + timeout;
    let mut records = BTreeMap::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let node_id = info
                    .get_property_val_str("node_id")
                    .unwrap_or(info.get_fullname())
                    .trim_end_matches(OPERON_MDNS_SERVICE)
                    .trim_end_matches('.')
                    .to_string();
                let fallback_endpoint = info
                    .get_addresses_v4()
                    .into_iter()
                    .next()
                    .map(|addr| format!("http://{}:{}", addr, info.get_port()))
                    .unwrap_or_else(|| {
                        format!("http://{}:{}", info.get_hostname(), info.get_port())
                    });
                let endpoint = info
                    .get_property_val_str("endpoint")
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or(fallback_endpoint);
                let capabilities = info
                    .get_property_val_str("capabilities")
                    .unwrap_or("")
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect();
                records.insert(
                    node_id.clone(),
                    DiscoveryRecord {
                        node_id,
                        endpoint,
                        provider: "lan".to_string(),
                        capabilities,
                    },
                );
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
    let list = DiscoveryList {
        nodes: records.into_values().collect(),
    };
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
            operon_network::NodeConfig {
                endpoint: node.endpoint.clone(),
                provider: NetworkProviderKind::Lan,
                token: None,
            },
        );
    }
    fs::write(path, serde_yaml::to_string(&NodesConfig { nodes })?)?;
    Ok(())
}

fn mount_read_only(
    config_path: PathBuf,
    target: &str,
    destination: PathBuf,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    fs::create_dir_all(&destination)?;
    materialize_read_only(config_path, &target.node_id, &target.path, &destination)?;
    let manifest = serde_json::json!({
        "mode": "read-only-poc",
        "node_id": target.node_id,
        "path": target.path,
        "destination": destination,
        "cache": "one-shot materialized copy",
        "consistency": "no live sync",
    });
    fs::write(
        destination.join(".operon-mount.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    if output.json {
        print_json(&manifest)?;
    } else if !output.quiet {
        println!("{}", destination.display());
    }
    Ok(())
}

fn materialize_read_only(
    config_path: PathBuf,
    node_id: &str,
    remote_path: &str,
    local_path: &Path,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path.clone(), node_id)?;
    let list: FsList = http_get_json_endpoint(
        &endpoint,
        &format!("/fs/list?path={}", encode_path(remote_path)),
    )?;
    for entry in list.entries {
        let child_path = local_path.join(entry.name);
        if entry.is_dir {
            fs::create_dir_all(&child_path)?;
            materialize_read_only(config_path.clone(), node_id, &entry.path, &child_path)?;
        } else {
            http_get_bytes_to_file_endpoint(
                &endpoint,
                &format!("/fs/read-stream?path={}", encode_path(&entry.path)),
                &child_path,
            )?;
            set_readonly(&child_path)?;
        }
    }
    Ok(())
}

fn set_readonly(path: &Path) -> anyhow::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)?;
    Ok(())
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

fn http_get_bytes_to_file_endpoint(
    endpoint: &NodeEndpoint,
    path: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let mut file = fs::File::create(output)?;
    http_get_bytes_to_writer_endpoint(endpoint, path, &mut file)
}

fn http_get_bytes_to_writer_endpoint(
    endpoint: &NodeEndpoint,
    path: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let target = parse_http_endpoint(&endpoint.endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let auth = endpoint
        .token
        .as_deref()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: */*\r\n{auth}Content-Length: 0\r\n\r\n",
        target.host,
    );
    stream.write_all(request.as_bytes())?;
    copy_http_body_to_writer(&endpoint.endpoint, path, stream, writer)
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

fn http_post_file_endpoint<T: serde::de::DeserializeOwned>(
    endpoint: &NodeEndpoint,
    path: &str,
    file: &Path,
) -> anyhow::Result<T> {
    let target = parse_http_endpoint(&endpoint.endpoint)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let mut file = fs::File::open(file)?;
    let body_len = file.metadata()?.len();
    let auth = endpoint
        .token
        .as_deref()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n{auth}Content-Type: application/octet-stream\r\nContent-Length: {body_len}\r\n\r\n",
        target.host,
    );
    stream.write_all(request.as_bytes())?;
    std::io::copy(&mut file, &mut stream)?;
    let body = read_http_body(&endpoint.endpoint, path, stream)?;
    Ok(serde_json::from_slice(&body)?)
}

fn copy_http_body_to_writer(
    endpoint: &str,
    path: &str,
    mut stream: TcpStream,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];
    let split = loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            anyhow::bail!("invalid HTTP response from {endpoint}");
        }
        response.extend_from_slice(&buffer[..count]);
        if let Some(split) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            break split;
        }
    };
    let head = std::str::from_utf8(&response[..split])?;
    let status = head
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status from {endpoint}"))?;
    let body_start = split + 4;
    let chunked = head
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"));
    if !status.contains(" 200 ") {
        let mut body = response[body_start..].to_vec();
        stream.read_to_end(&mut body)?;
        anyhow::bail!(
            "request to {endpoint}{path} failed: {status}: {}",
            format_error_body(&String::from_utf8_lossy(&body))
        );
    }
    if chunked {
        return copy_chunked_http_body(&response[body_start..], stream, writer);
    }
    writer.write_all(&response[body_start..])?;
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
    }
    Ok(())
}

fn copy_chunked_http_body(
    initial: &[u8],
    mut stream: TcpStream,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let mut data = initial.to_vec();
    stream.read_to_end(&mut data)?;
    let mut cursor = 0;
    loop {
        let Some(line_end) = find_crlf(&data[cursor..]) else {
            anyhow::bail!("invalid chunked HTTP response");
        };
        let size_line = std::str::from_utf8(&data[cursor..cursor + line_end])?;
        let size_hex = size_line.split(';').next().unwrap_or(size_line).trim();
        let size = usize::from_str_radix(size_hex, 16)?;
        cursor += line_end + 2;
        if size == 0 {
            break;
        }
        if data.len() < cursor + size + 2 {
            anyhow::bail!("truncated chunked HTTP response");
        }
        writer.write_all(&data[cursor..cursor + size])?;
        cursor += size + 2;
    }
    Ok(())
}

fn find_crlf(data: &[u8]) -> Option<usize> {
    data.windows(2).position(|window| window == b"\r\n")
}

fn read_http_body(endpoint: &str, path: &str, stream: TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    copy_http_body_to_writer(endpoint, path, stream, &mut body)?;
    Ok(body)
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

fn format_service_protocol(protocol: &ServiceProtocol) -> &'static str {
    match protocol {
        ServiceProtocol::Tcp => "tcp",
    }
}

fn print_json(value: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
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
