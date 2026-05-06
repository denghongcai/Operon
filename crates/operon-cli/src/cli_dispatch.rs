use std::{io, time::Duration};

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use operon_config::OperonConfig;

use crate::{
    cli_args::{
        Args, AuditCommand, CapabilityCommand, Command, ConfigCommand, ExecCommand, FsCommand,
        GraphCommand, InitCommand, NodeCommand, ServiceCommand, TraceCommand,
    },
    commands, graph, onboard,
    output::OutputMode,
};

pub(crate) async fn dispatch(args: Args) -> anyhow::Result<()> {
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
        Command::Doctor {
            node,
            mount_runtime,
        } => commands::doctor::run(config_path, node, mount_runtime, output).await,
        Command::Completion { shell } => completion(shell),
    }
}

fn completion(shell: Shell) -> anyhow::Result<()> {
    let mut command = Args::command();
    generate(shell, &mut command, "operon", &mut io::stdout());
    Ok(())
}
