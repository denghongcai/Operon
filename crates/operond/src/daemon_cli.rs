use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "operond", version, about = "Operon capability daemon")]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    Start(StartArgs),
    #[command(about = "Install and control operond through the platform service manager")]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ServiceCommand {
    #[command(about = "Install a platform-native operond service entry")]
    Install(ServiceInstallArgs),
    #[command(about = "Start the installed operond service")]
    Start,
    #[command(about = "Stop the installed operond service")]
    Stop,
    #[command(about = "Show the installed operond service status")]
    Status,
    #[command(about = "Uninstall the platform-native operond service entry")]
    Uninstall,
    #[cfg(any(test, windows))]
    #[command(
        hide = true,
        about = "Run operond under the Windows Service Control Manager"
    )]
    Run(ServiceRunArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct StartArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub(crate) struct ServiceInstallArgs {
    #[arg(long)]
    pub(crate) config: PathBuf,
}

#[cfg(any(test, windows))]
#[derive(Debug, Parser)]
pub(crate) struct ServiceRunArgs {
    #[arg(long)]
    pub(crate) config: PathBuf,
}
