use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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
        #[command(flatten)]
        options: ServiceOptions,
        #[command(subcommand)]
        command: ServiceCommand,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum ServiceBackend {
    #[default]
    Auto,
    Systemd,
    Openrc,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ServiceOptions {
    /// Linux service backend; auto detects the init environment, not libc.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub(crate) backend: ServiceBackend,
    /// OpenRC command/readiness deadline in seconds; 0 disables the deadline.
    #[arg(long, global = true, default_value_t = 60, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) timeout_secs: u64,
    /// OpenRC stop command deadline; install also sets supervisor grace (0 disables).
    #[arg(long, global = true, default_value_t = 30, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) stop_timeout_secs: u64,
    /// Emit structured OpenRC service results.
    #[arg(long, global = true)]
    pub(crate) json: bool,
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
    /// Wait for active exec/process-group cleanup on shutdown; 0 waits indefinitely.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) shutdown_timeout_secs: u64,
}

#[derive(Debug, Parser)]
pub(crate) struct ServiceInstallArgs {
    #[arg(long)]
    pub(crate) config: PathBuf,
    /// Existing non-root account required for an OpenRC system service.
    #[arg(long)]
    pub(crate) service_user: Option<String>,
    /// OpenRC daemon's graceful shutdown deadline; 0 waits indefinitely.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) shutdown_timeout_secs: u64,
    /// OpenRC delay between restart attempts in seconds.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) respawn_delay_secs: u64,
    /// OpenRC maximum restarts per period; 0 means unlimited.
    #[arg(long, default_value_t = 5)]
    pub(crate) respawn_max: u32,
    /// OpenRC restart accounting period in seconds; 0 disables the period.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(0..=604800))]
    pub(crate) respawn_period_secs: u64,
}

#[cfg(any(test, windows))]
#[derive(Debug, Parser)]
pub(crate) struct ServiceRunArgs {
    #[arg(long)]
    pub(crate) config: PathBuf,
}
