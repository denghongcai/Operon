use std::{io, net::SocketAddr, path::PathBuf, time::Duration};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use operon_config::OperonConfig;

mod commands;
mod graph;
mod grpc;
mod grpc_audit;
mod grpc_exec;
mod grpc_exec_api;
mod grpc_fs;
mod grpc_service;
mod grpc_service_api;
mod onboard;
mod output;
mod private_files;
mod target;

use output::{print_json, OutputMode};

#[derive(Debug, Parser)]
#[command(
    name = "operon",
    version,
    about = "Operate Operon nodes through config.yaml, gRPC runtime APIs, and policy-aware capabilities"
)]
struct Args {
    /// Path to config.yaml. Defaults to $HOME/.operon/config.yaml.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Render command output as JSON for scripts.
    #[arg(long)]
    json: bool,

    /// Suppress non-essential human output.
    #[arg(long)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Inspect and validate configured nodes")]
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    #[command(about = "Create starter Operon configuration files")]
    Init {
        #[command(subcommand)]
        command: InitCommand,
    },
    #[command(about = "Interactively create a usable local Operon configuration")]
    Onboard(onboard::OnboardArgs),
    #[command(about = "Inspect policy-allowed runtime capabilities")]
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    #[command(about = "Read and mutate remote filesystem capabilities")]
    Fs {
        #[command(subcommand)]
        command: FsCommand,
    },
    #[command(about = "Inspect audit events emitted by a node")]
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    #[command(about = "Inspect service metadata, health, and local forwards")]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    #[command(about = "Run execs and stream exec stdin/stdout/stderr")]
    Exec {
        #[command(subcommand)]
        command: ExecCommand,
    },
    #[command(about = "Run an execution graph YAML file")]
    Run {
        /// Execution graph YAML path.
        workflow: PathBuf,
        /// Optional path for the execution trace JSON output.
        #[arg(long)]
        trace_output: Option<PathBuf>,
    },
    #[command(about = "Run or inspect execution graphs")]
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    #[command(about = "Run workflow files through the execution graph runner")]
    Workflow {
        #[command(subcommand)]
        command: GraphCommand,
    },
    #[command(about = "Inspect execution trace files")]
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
    #[command(about = "Mount a remote filesystem capability on Linux")]
    Mount {
        /// Remote target in node:/path form.
        target: String,
        /// Local mount point.
        #[arg(long)]
        to: PathBuf,
    },
    #[command(about = "Explain the active Operon config.yaml without reading raw YAML")]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    #[command(about = "Run config, endpoint, auth, protocol, capability, and service diagnostics")]
    Doctor {
        /// Optional node ids to diagnose. Defaults to every configured client node.
        #[arg(long)]
        node: Vec<String>,
    },
    #[command(about = "Generate shell completion scripts")]
    Completion {
        /// Shell to generate completions for.
        shell: Shell,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    #[command(about = "List client nodes configured in config.yaml")]
    List,
    #[command(about = "Discover LAN nodes with mDNS")]
    Discover {
        /// Discovery timeout in seconds.
        #[arg(long, default_value_t = 3)]
        timeout_secs: u64,
        /// Check discovered endpoint health before printing results.
        #[arg(long)]
        check_health: bool,
        /// Optional YAML file to write discovered client nodes into.
        #[arg(long)]
        output_config: Option<PathBuf>,
    },
    #[command(about = "Resolve a configured node to its endpoint")]
    Resolve {
        /// Node id from config.yaml.
        node_id: String,
    },
    #[command(about = "Call runtime health and node info on a configured node")]
    Ping {
        /// Node id from config.yaml.
        node_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum InitCommand {
    #[command(about = "Create a starter config.yaml plus referenced token and secrets files")]
    Config {
        /// Path for the new config.yaml.
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(about = "Summarize daemon, client, auth, policy, services, and secrets settings")]
    Explain,
}

#[derive(Debug, Subcommand)]
enum CapabilityCommand {
    #[command(about = "List capabilities exposed by a node")]
    List {
        /// Node id from config.yaml.
        node_id: String,
    },
    #[command(about = "Explain why one capability action is allowed or denied")]
    Explain {
        /// Node id from config.yaml.
        node_id: String,
        /// Capability id, for example fs:workspace, exec:default, secret:default, or service:web.
        capability_id: String,
        /// Action to diagnose, for example read, write, run, use, check, or forward.
        action: String,
        /// Resource to diagnose, such as a path, cwd, secret name, or service id.
        resource: String,
        /// Optional timeout in seconds for exec run diagnostics.
        #[arg(long)]
        timeout_secs: Option<u64>,
    },
}

#[derive(Debug, Subcommand)]
enum FsCommand {
    #[command(about = "Stat a remote file or directory")]
    Stat {
        /// Remote target in node:/path form.
        target: String,
    },
    #[command(about = "List a remote directory")]
    List {
        /// Remote target in node:/path form.
        target: String,
    },
    #[command(about = "Stream a remote file to stdout or a local file")]
    Read {
        /// Remote target in node:/path form.
        target: String,
        /// Local output file. Omit to write content to stdout or JSON.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    #[command(about = "Write local content or a local file to a remote path")]
    Write {
        /// Remote target in node:/path form.
        target: String,
        /// Inline content to write.
        #[arg(long)]
        content: Option<String>,
        /// Local file whose bytes should be streamed to the target.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Only write if the current remote file version matches this value.
        #[arg(long)]
        expected_version: Option<String>,
    },
    #[command(about = "Create a remote directory")]
    Mkdir {
        /// Remote target in node:/path form.
        target: String,
    },
    #[command(about = "Remove a remote file or directory")]
    Rm {
        /// Remote target in node:/path form.
        target: String,
    },
    #[command(about = "Rename or move a remote path")]
    Rename {
        /// Source target in node:/path form.
        from: String,
        /// Destination target in node:/path form.
        to: String,
    },
    #[command(about = "Copy a remote file or directory")]
    Copy {
        /// Source target in node:/path form.
        from: String,
        /// Destination target in node:/path form.
        to: String,
    },
    #[command(about = "Resize a remote file")]
    Truncate {
        /// Remote target in node:/path form.
        target: String,
        /// New file size in bytes.
        #[arg(long)]
        size: u64,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    #[command(about = "List audit events for a node")]
    List {
        /// Node id from config.yaml.
        node_id: String,
    },
    #[command(about = "Show audit events with optional filters")]
    Show {
        /// Node id from config.yaml.
        node_id: String,
        /// Maximum number of events to show.
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by capability id.
        #[arg(long)]
        capability: Option<String>,
        /// Filter by action name.
        #[arg(long)]
        action: Option<String>,
        /// Filter by authorization outcome.
        #[arg(long)]
        allowed: Option<bool>,
        /// Filter by resource substring.
        #[arg(long)]
        resource: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    #[command(about = "List service metadata exposed by a node")]
    List {
        /// Node id from config.yaml.
        node_id: String,
    },
    #[command(about = "Run a policy-aware health check for a service")]
    Check {
        /// Node id from config.yaml.
        node_id: String,
        /// Service id from policy.service.services.
        service_id: String,
    },
    #[command(about = "Forward a local TCP port to a policy-allowed remote service")]
    Forward {
        /// Node id from config.yaml.
        node_id: String,
        /// Service id from policy.service.services.
        service_id: String,
        /// Local socket address to listen on, for example 127.0.0.1:8080.
        #[arg(long)]
        listen: SocketAddr,
    },
    #[command(about = "Forward UDP datagrams to a policy-allowed remote UDP service")]
    ForwardUdp {
        /// Node id from config.yaml.
        node_id: String,
        /// Service id from policy.service.services.
        service_id: String,
        /// Local UDP socket address to listen on, for example 127.0.0.1:5353.
        #[arg(long)]
        listen: SocketAddr,
    },
}

#[derive(Debug, Subcommand)]
enum ExecCommand {
    #[command(about = "Run a shell command on a node")]
    Run {
        /// Node id from config.yaml.
        node_id: String,
        /// Remote working directory allowed by policy.
        #[arg(long)]
        cwd: Option<String>,
        /// Exec timeout in seconds.
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
        /// Secret name to inject when allowed by policy.
        #[arg(long)]
        secret: Vec<String>,
        /// Return after the exec is accepted instead of waiting for completion.
        #[arg(long)]
        detach: bool,
        /// Execute CLI words as argv without shell parsing.
        #[arg(long)]
        argv: bool,
        /// Shell command to execute. Multiple CLI words are shell-escaped.
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    #[command(about = "List execs known by a node")]
    List {
        /// Node id from config.yaml.
        node_id: String,
    },
    #[command(about = "Get a exec status record")]
    Status {
        /// Node id from config.yaml.
        node_id: String,
        /// Exec id returned by exec run or exec list.
        exec_id: String,
    },
    #[command(about = "Read or follow exec stdout/stderr logs")]
    Logs {
        /// Node id from config.yaml.
        node_id: String,
        /// Exec id returned by exec run or exec list.
        exec_id: String,
        /// Keep following log output.
        #[arg(long)]
        follow: bool,
        /// Use the streaming log RPC.
        #[arg(long)]
        stream: bool,
    },
    #[command(about = "Write stdin bytes to a running exec")]
    Stdin {
        /// Node id from config.yaml.
        node_id: String,
        /// Exec id returned by exec run or exec list.
        exec_id: String,
        /// Inline stdin content.
        #[arg(long)]
        content: Option<String>,
        /// Local file whose bytes should be streamed to exec stdin.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Close exec stdin after optional content or file bytes.
        #[arg(long)]
        close: bool,
    },
    #[command(about = "Open a PTY-backed interactive exec session")]
    Session {
        /// Node id from config.yaml.
        node_id: String,
        /// Remote working directory allowed by policy.
        #[arg(long)]
        cwd: Option<String>,
        /// Session timeout in seconds.
        #[arg(long, default_value_t = 300)]
        timeout_secs: u64,
        /// Secret name to inject when allowed by policy.
        #[arg(long)]
        secret: Vec<String>,
        /// Execute CLI words as argv without shell parsing.
        #[arg(long)]
        argv: bool,
        /// Initial terminal rows. Defaults to the local TTY rows, or 24.
        #[arg(long)]
        rows: Option<u16>,
        /// Initial terminal columns. Defaults to the local TTY columns, or 80.
        #[arg(long)]
        cols: Option<u16>,
        /// Optional stdin content. Omit to stream local stdin interactively.
        #[arg(long)]
        content: Option<String>,
        /// Shell command to execute in the PTY. Multiple CLI words are shell-escaped.
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    #[command(about = "Cancel a running exec")]
    Cancel {
        /// Node id from config.yaml.
        node_id: String,
        /// Exec id returned by exec run or exec list.
        exec_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    #[command(about = "Show one execution trace JSON file")]
    Show {
        /// Path to a trace JSON file.
        path: PathBuf,
    },
    #[command(about = "List execution trace files under a directory")]
    List {
        /// Directory to scan.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum GraphCommand {
    #[command(about = "Run an execution graph YAML file")]
    Run {
        /// Execution graph YAML path.
        workflow: PathBuf,
        /// Optional path for the execution trace JSON output.
        #[arg(long)]
        trace_output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config_path = args.config.unwrap_or_else(OperonConfig::default_path);
    let output = OutputMode {
        json: args.json,
        quiet: args.quiet,
    };

    match args.command {
        Command::Node { command } => match command {
            NodeCommand::List => commands::node::list(config_path, output),
            NodeCommand::Discover {
                timeout_secs,
                check_health,
                output_config,
            } => {
                commands::node::discover(
                    Duration::from_secs(timeout_secs),
                    output_config,
                    check_health,
                    output,
                )
                .await
            }
            NodeCommand::Resolve { node_id } => {
                commands::node::resolve(config_path, &node_id, output)
            }
            NodeCommand::Ping { node_id } => {
                commands::node::ping(config_path, &node_id, output).await
            }
        },
        Command::Init { command } => match command {
            InitCommand::Config { path } => commands::init::init_config(path, output),
        },
        Command::Onboard(args) => onboard::run(args, output),
        Command::Capability { command } => match command {
            CapabilityCommand::List { node_id } => {
                commands::capability::list(config_path, &node_id, output).await
            }
            CapabilityCommand::Explain {
                node_id,
                capability_id,
                action,
                resource,
                timeout_secs,
            } => {
                commands::capability::explain(
                    config_path,
                    &node_id,
                    operon_core::CapabilityDiagnosticRequest {
                        capability_id,
                        action,
                        resource,
                        timeout_secs,
                    },
                    output,
                )
                .await
            }
        },
        Command::Fs { command } => match command {
            FsCommand::Stat { target } => commands::fs::stat(config_path, &target, output).await,
            FsCommand::List { target } => commands::fs::list(config_path, &target, output).await,
            FsCommand::Read {
                target,
                output: file_output,
            } => commands::fs::read(config_path, &target, file_output, output).await,
            FsCommand::Write {
                target,
                content,
                file,
                expected_version,
            } => {
                commands::fs::write(
                    config_path,
                    &target,
                    content,
                    file,
                    expected_version,
                    output,
                )
                .await
            }
            FsCommand::Mkdir { target } => commands::fs::mkdir(config_path, &target, output).await,
            FsCommand::Rm { target } => commands::fs::rm(config_path, &target, output).await,
            FsCommand::Rename { from, to } => {
                commands::fs::rename(config_path, &from, &to, output).await
            }
            FsCommand::Copy { from, to } => {
                commands::fs::copy(config_path, &from, &to, output).await
            }
            FsCommand::Truncate { target, size } => {
                commands::fs::truncate(config_path, &target, size, output).await
            }
        },
        Command::Audit { command } => match command {
            AuditCommand::List { node_id } => {
                commands::audit::list(config_path, &node_id, output).await
            }
            AuditCommand::Show {
                node_id,
                limit,
                capability,
                action,
                allowed,
                resource,
            } => {
                let filter = commands::audit::AuditFilter {
                    limit,
                    capability,
                    action,
                    allowed,
                    resource,
                };
                commands::audit::show(config_path, &node_id, filter, output).await
            }
        },
        Command::Service { command } => match command {
            ServiceCommand::List { node_id } => {
                commands::service::list(config_path, &node_id, output).await
            }
            ServiceCommand::Check {
                node_id,
                service_id,
            } => commands::service::check(config_path, &node_id, &service_id, output).await,
            ServiceCommand::Forward {
                node_id,
                service_id,
                listen,
            } => commands::service::forward(config_path, node_id, service_id, listen, output).await,
            ServiceCommand::ForwardUdp {
                node_id,
                service_id,
                listen,
            } => {
                commands::service::forward_udp(config_path, node_id, service_id, listen, output)
                    .await
            }
        },
        Command::Exec { command } => match command {
            ExecCommand::Run {
                node_id,
                cwd,
                timeout_secs,
                secret,
                detach,
                argv,
                command,
            } => {
                commands::exec::run(commands::exec::ExecRunInput {
                    config_path,
                    node_id,
                    cwd,
                    timeout_secs,
                    secrets: secret,
                    detach,
                    argv,
                    command,
                    output,
                })
                .await
            }
            ExecCommand::List { node_id } => {
                commands::exec::list(config_path, &node_id, output).await
            }
            ExecCommand::Status { node_id, exec_id } => {
                commands::exec::status(config_path, &node_id, &exec_id, output).await
            }
            ExecCommand::Logs {
                node_id,
                exec_id,
                follow,
                stream,
            } => {
                commands::exec::logs(config_path, &node_id, &exec_id, follow, stream, output).await
            }
            ExecCommand::Stdin {
                node_id,
                exec_id,
                content,
                file,
                close,
            } => {
                commands::exec::stdin(
                    config_path,
                    &node_id,
                    &exec_id,
                    content,
                    file,
                    close,
                    output,
                )
                .await
            }
            ExecCommand::Session {
                node_id,
                cwd,
                timeout_secs,
                secret,
                argv,
                rows,
                cols,
                content,
                command,
            } => {
                commands::exec_session::session(commands::exec_session::ExecSessionInput {
                    config_path,
                    node_id,
                    cwd,
                    timeout_secs,
                    secrets: secret,
                    argv,
                    rows,
                    cols,
                    content,
                    command,
                    output,
                })
                .await
            }
            ExecCommand::Cancel { node_id, exec_id } => {
                commands::exec::cancel(config_path, &node_id, &exec_id, output).await
            }
        },
        Command::Run {
            workflow,
            trace_output,
        } => graph::run_graph(config_path, workflow, trace_output).await,
        Command::Graph { command } | Command::Workflow { command } => match command {
            GraphCommand::Run {
                workflow,
                trace_output,
            } => graph::run_graph(config_path, workflow, trace_output).await,
        },
        Command::Trace { command } => match command {
            TraceCommand::Show { path } => commands::trace::show(path, output),
            TraceCommand::List { dir } => commands::trace::list(dir, output),
        },
        Command::Mount { target, to } => {
            commands::mount::mount_fs(config_path, &target, to, output)
        }
        Command::Config { command } => match command {
            ConfigCommand::Explain => commands::config::explain(config_path, output),
        },
        Command::Doctor { node } => commands::doctor::run(config_path, node, output).await,
        Command::Completion { shell } => completion(shell),
    }
}

fn completion(shell: Shell) -> anyhow::Result<()> {
    let mut command = Args::command();
    generate(shell, &mut command, "operon", &mut io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_model_exposes_completion_command() {
        let mut command = Args::command();

        command
            .find_subcommand_mut("completion")
            .expect("completion subcommand should exist");
    }

    #[test]
    fn clap_model_exposes_graph_and_workflow_run_aliases() {
        let mut command = Args::command();

        command
            .find_subcommand_mut("graph")
            .expect("graph subcommand should exist")
            .find_subcommand_mut("run")
            .expect("graph run subcommand should exist");
        command
            .find_subcommand_mut("workflow")
            .expect("workflow subcommand should exist")
            .find_subcommand_mut("run")
            .expect("workflow run subcommand should exist");
    }

    #[test]
    fn clap_model_exposes_capability_explain_command() {
        let mut command = Args::command();

        command
            .find_subcommand_mut("capability")
            .expect("capability subcommand should exist")
            .find_subcommand_mut("explain")
            .expect("capability explain subcommand should exist");
    }

    #[test]
    fn clap_model_exposes_doctor_command() {
        let mut command = Args::command();

        command
            .find_subcommand_mut("doctor")
            .expect("doctor subcommand should exist");
    }

    #[test]
    fn clap_model_exposes_exec_session_command() {
        let mut command = Args::command();

        command
            .find_subcommand_mut("exec")
            .expect("exec subcommand should exist")
            .find_subcommand_mut("session")
            .expect("exec session subcommand should exist");
    }
}
