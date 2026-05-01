use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use operon_config::{NetworkProviderKind, NodeConfig, OperonConfig};
use operon_core::{
    AuditLog, CapabilityList, DiscoveryList, DiscoveryRecord, ExecutionTrace, FsList, FsRead,
    FsWrite, HealthStatus, JobList, JobRecord, JobRunRequest, JobStatus, JobStdin, JobStdinClose,
    NodeInfo, ServiceCheck, ServiceList, ServiceProtocol, TraceFile, TraceFileList,
};

mod graph;
mod grpc;
mod onboard;

const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

#[derive(Debug, Parser)]
#[command(name = "operon", about = "Operon CLI")]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,

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
    Onboard(onboard::OnboardArgs),
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
        target: String,
        #[arg(long)]
        to: PathBuf,
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
    Mkdir {
        target: String,
    },
    Rm {
        target: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Copy {
        from: String,
        to: String,
    },
    Truncate {
        target: String,
        #[arg(long)]
        size: u64,
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputMode {
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
    let config_path = args.config.unwrap_or_else(OperonConfig::default_path);
    let output = OutputMode {
        json: args.json,
        quiet: args.quiet,
    };

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => list_nodes(config_path, output),
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
            NodeCommand::Resolve { node_id } => resolve_node(config_path, &node_id, output),
            NodeCommand::Ping { node_id } => ping_node(config_path, &node_id, output),
        },
        Command::Init { command } => match command {
            InitCommand::Config { path } => init_config(path, output),
        },
        Command::Onboard(args) => onboard::run(args, output),
        Command::Provider { command } => match command {
            ProviderCommand::List => list_providers(output),
        },
        Command::Capability { command } => match command {
            CapabilityCommand::List { node_id } => list_capabilities(config_path, &node_id, output),
        },
        Command::Fs { command } => match command {
            FsCommand::Stat { target } => fs_stat(config_path, &target, output),
            FsCommand::List { target } => fs_list(config_path, &target, output),
            FsCommand::Read {
                target,
                output: file_output,
            } => fs_read(config_path, &target, file_output, output),
            FsCommand::Write {
                target,
                content,
                file,
            } => fs_write(config_path, &target, content, file, output),
            FsCommand::Mkdir { target } => fs_mkdir(config_path, &target, output),
            FsCommand::Rm { target } => fs_rm(config_path, &target, output),
            FsCommand::Rename { from, to } => fs_rename(config_path, &from, &to, output),
            FsCommand::Copy { from, to } => fs_copy(config_path, &from, &to, output),
            FsCommand::Truncate { target, size } => fs_truncate(config_path, &target, size, output),
        },
        Command::Audit { command } => match command {
            AuditCommand::List { node_id } => list_audit(config_path, &node_id, output),
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
                audit_show(config_path, &node_id, filter, output)
            }
        },
        Command::Service { command } => match command {
            ServiceCommand::List { node_id } => service_list(config_path, &node_id, output),
            ServiceCommand::Check {
                node_id,
                service_id,
            } => service_check(config_path, &node_id, &service_id, output),
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
                config_path,
                node_id,
                cwd,
                timeout_secs,
                secrets: secret,
                detach,
                command,
                output,
            }),
            JobCommand::List { node_id } => job_list(config_path, &node_id, output),
            JobCommand::Status { node_id, job_id } => {
                job_status(config_path, &node_id, &job_id, output)
            }
            JobCommand::Logs {
                node_id,
                job_id,
                follow,
                stream,
            } => job_logs(config_path, &node_id, &job_id, follow, stream),
            JobCommand::Stdin {
                node_id,
                job_id,
                content,
                file,
                close,
            } => job_stdin(config_path, &node_id, &job_id, content, file, close, output),
            JobCommand::Cancel { node_id, job_id } => {
                job_cancel(config_path, &node_id, &job_id, output)
            }
        },
        Command::Run {
            workflow,
            trace_output,
        } => graph::run_graph(config_path, workflow, trace_output),
        Command::Trace { command } => match command {
            TraceCommand::Show { path } => trace_show(path, output),
            TraceCommand::List { dir } => trace_list(dir, output),
        },
        Command::Mount { target, to } => mount_fs(config_path, &target, to, output),
    }
}

fn list_nodes(config_path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
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

fn ping_node(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;

    let (health, node): (HealthStatus, NodeInfo) = grpc::health_and_node(&endpoint)?;
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
    let endpoint = load_endpoint(config_path, node_id)?;

    let list: CapabilityList = grpc::list_capabilities(&endpoint)?;
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
    let stat = grpc::fs_stat(&endpoint, &target.path)?;
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
    let list: FsList = grpc::fs_list(&endpoint, &target.path)?;
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
        let mut file = fs::File::create(&file_output)?;
        grpc::read_file_to_writer(&endpoint, &target.path, &mut file)?;
    } else {
        let mut content = Vec::new();
        grpc::read_file_to_writer(&endpoint, &target.path, &mut content)?;
        let read = FsRead {
            path: target.path.clone(),
            content: String::from_utf8(content)?,
        };
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
            grpc::write_file_bytes(&endpoint, &target.path, content.as_bytes())?
        }
        (None, Some(file)) => grpc::write_file(&endpoint, &target.path, &file)?,
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

fn fs_mkdir(config_path: PathBuf, target: &str, output: OutputMode) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat = grpc::fs_mkdir(&endpoint, &target.path)?;
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

fn fs_rm(config_path: PathBuf, target: &str, output: OutputMode) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let path = grpc::fs_delete(&endpoint, &target.path)?;
    let result = serde_json::json!({ "path": path });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} deleted=true",
        target.node_id,
        result["path"].as_str().unwrap_or_default()
    );
    Ok(())
}

fn fs_rename(config_path: PathBuf, from: &str, to: &str, output: OutputMode) -> anyhow::Result<()> {
    let from = parse_node_path(from)?;
    let to = parse_node_path(to)?;
    if from.node_id != to.node_id {
        anyhow::bail!("fs rename requires source and target to use the same node");
    }
    let endpoint = load_endpoint(config_path, &from.node_id)?;
    let (from_path, to_path) = grpc::fs_rename(&endpoint, &from.path, &to.path)?;
    let result = serde_json::json!({
        "from_path": from_path,
        "to_path": to_path,
    });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} -> {}",
        from.node_id,
        result["from_path"].as_str().unwrap_or_default(),
        result["to_path"].as_str().unwrap_or_default()
    );
    Ok(())
}

fn fs_copy(config_path: PathBuf, from: &str, to: &str, output: OutputMode) -> anyhow::Result<()> {
    let from = parse_node_path(from)?;
    let to = parse_node_path(to)?;
    if from.node_id != to.node_id {
        anyhow::bail!("fs copy requires source and target to use the same node");
    }
    let endpoint = load_endpoint(config_path, &from.node_id)?;
    let (from_path, to_path, bytes_copied) = grpc::fs_copy(&endpoint, &from.path, &to.path)?;
    let result = serde_json::json!({
        "from_path": from_path,
        "to_path": to_path,
        "bytes_copied": bytes_copied,
    });
    if output.json {
        print_json(&result)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{}:{} -> {} bytes_copied={}",
        from.node_id,
        result["from_path"].as_str().unwrap_or_default(),
        result["to_path"].as_str().unwrap_or_default(),
        result["bytes_copied"].as_u64().unwrap_or_default()
    );
    Ok(())
}

fn fs_truncate(
    config_path: PathBuf,
    target: &str,
    size: u64,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let stat = grpc::fs_truncate(&endpoint, &target.path, size)?;
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

fn list_audit(config_path: PathBuf, node_id: &str, output: OutputMode) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = grpc::list_audit(&endpoint)?;
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
    let audit: AuditLog = grpc::list_audit(&endpoint)?;
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
                .is_none_or(|capability| &event.capability == capability)
                && filter
                    .action
                    .as_ref()
                    .is_none_or(|action| &event.action == action)
                && filter
                    .allowed
                    .is_none_or(|allowed| event.allowed == allowed)
                && filter
                    .resource
                    .as_ref()
                    .is_none_or(|resource| event.resource.contains(resource))
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
    let list: ServiceList = grpc::list_services(&endpoint)?;
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
    let check: ServiceCheck = grpc::check_service(&endpoint, service_id)?;
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
    let record: JobRecord = grpc::run_job(&endpoint, request)?;
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
    let list: JobList = grpc::list_jobs(&endpoint)?;
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
        return grpc::stream_job_logs_to_writer(&endpoint, job_id, &mut std::io::stdout());
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
        let closed: JobStdinClose = grpc::close_job_stdin(&endpoint, job_id)?;
        if output.json {
            print_json(&closed)?;
        } else if !output.quiet {
            println!("{} stdin_closed={}", closed.job_id, closed.closed);
        }
        return Ok(());
    }
    let written: JobStdin = match (content, file) {
        (Some(content), None) => {
            grpc::write_job_stdin_bytes(&endpoint, job_id, content.as_bytes())?
        }
        (None, Some(file)) => grpc::write_job_stdin_file(&endpoint, job_id, &file)?,
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
    let record: JobRecord = grpc::cancel_job(&endpoint, job_id)?;
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
    let content = r#"version: 1

daemon:
  node_id: local
  grpc_listen: 127.0.0.1:7789
  workspace: /workspace
  advertise_lan: false
  store: store.jsonl
  auth:
    token_file: token

client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      provider: manual
      auth:
        token_file: token

policy:
  subject: local-cli
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
        port: 7789
        protocol: tcp
        description: Operon gRPC daemon listener

secrets:
  file: secrets.yaml
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
                    .map(|addr| format!("grpc://{}:{}", addr, info.get_port()))
                    .unwrap_or_else(|| {
                        format!("grpc://{}:{}", info.get_hostname(), info.get_port())
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

fn mount_fs(
    config_path: PathBuf,
    target: &str,
    destination: PathBuf,
    output: OutputMode,
) -> anyhow::Result<()> {
    let target = parse_node_path(target)?;
    let endpoint = load_endpoint(config_path, &target.node_id)?;
    let mount = operon_mount::spawn_mount(operon_mount::MountOptions {
        endpoint,
        remote_path: target.path.clone(),
        mount_point: destination.clone(),
    })?;
    let manifest = serde_json::json!({
        "mode": "write-through-live-fuse",
        "node_id": target.node_id,
        "path": target.path,
        "destination": destination,
        "cache": "kernel page cache only",
        "consistency": "live reads and write-through mutations through Operon fs gRPC; metadata cached for one second",
        "write": "single-writer write-through in v0.6.1",
    });
    if output.json {
        print_json(&manifest)?;
    } else if !output.quiet {
        println!(
            "mounted {}:{} at {}",
            manifest["node_id"].as_str().unwrap_or_default(),
            manifest["path"].as_str().unwrap_or_default(),
            manifest["destination"].as_str().unwrap_or_default()
        );
        println!("press Ctrl-C to unmount");
    }
    mount.wait_for_shutdown()
}

pub(crate) fn load_job(
    config_path: PathBuf,
    node_id: &str,
    job_id: &str,
) -> anyhow::Result<JobRecord> {
    let endpoint = load_endpoint(config_path, node_id)?;
    grpc::get_job(&endpoint, job_id)
}

fn load_job_from_path(
    config_path: PathBuf,
    node_id: &str,
    path: &str,
) -> anyhow::Result<JobRecord> {
    let job_id = path
        .split_once("id=")
        .map(|(_, id)| id)
        .ok_or_else(|| anyhow::anyhow!("job path must include id query"))?;
    load_job(config_path, node_id, job_id)
}

fn print_job_status(record: &JobRecord) {
    println!(
        "{} {} {:?} exit_code={:?}",
        record.node_id, record.id, record.status, record.exit_code
    );
}

fn format_service_protocol(protocol: &ServiceProtocol) -> &'static str {
    match protocol {
        ServiceProtocol::Tcp => "tcp",
    }
}

pub(crate) fn print_json(value: &impl serde::Serialize) -> anyhow::Result<()> {
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
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    config.endpoint(node_id, &config_dir)
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
}
