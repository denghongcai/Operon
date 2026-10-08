//! Linux init selection is independent of the binary's libc target.
use std::{env, path::Path};

use crate::daemon_cli::ServiceBackend;

fn available(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| {
            directory.join(program).metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
    })
}

pub(crate) fn select_backend(requested: ServiceBackend) -> anyhow::Result<ServiceBackend> {
    select(
        requested,
        available("systemctl"),
        available("rc-service") && available("rc-update") && available("supervise-daemon"),
        Path::new("/run/systemd/system").is_dir(),
        Path::new("/run/openrc/softlevel").is_file(),
    )
}

fn select(
    requested: ServiceBackend,
    systemd_tools: bool,
    openrc_tools: bool,
    systemd_running: bool,
    openrc_running: bool,
) -> anyhow::Result<ServiceBackend> {
    match requested {
        ServiceBackend::Systemd if systemd_tools => Ok(requested),
        ServiceBackend::Openrc if openrc_tools => Ok(requested),
        ServiceBackend::Systemd => anyhow::bail!("systemctl is missing; install systemd or use foreground operond start"),
        ServiceBackend::Openrc => anyhow::bail!("OpenRC requires rc-service, rc-update and supervise-daemon; install OpenRC or use foreground operond start"),
        ServiceBackend::Auto => match (systemd_running && systemd_tools, openrc_running && openrc_tools) {
            (true, false) => Ok(ServiceBackend::Systemd),
            (false, true) => Ok(ServiceBackend::Openrc),
            (true, true) => anyhow::bail!("both systemd and OpenRC are active; select --backend explicitly"),
            (false, false) => match (systemd_tools, openrc_tools) {
                (true, false) => Ok(ServiceBackend::Systemd),
                (false, true) => Ok(ServiceBackend::Openrc),
                (true, true) => anyhow::bail!("Linux init environment is ambiguous; select --backend systemd or --backend openrc"),
                _ => anyhow::bail!("no Linux service manager found; use foreground operond start or install a service manager"),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_uses_environment_not_libc() {
        assert_eq!(
            select(ServiceBackend::Auto, true, false, false, false).unwrap(),
            ServiceBackend::Systemd
        );
        assert_eq!(
            select(ServiceBackend::Auto, false, true, false, true).unwrap(),
            ServiceBackend::Openrc
        );
        assert_eq!(
            select(ServiceBackend::Auto, true, true, false, true).unwrap(),
            ServiceBackend::Openrc
        );
        assert!(select(ServiceBackend::Auto, true, true, false, false).is_err());
        assert!(select(ServiceBackend::Auto, true, true, true, true).is_err());
        assert!(select(ServiceBackend::Auto, false, false, false, false).is_err());
        assert!(select(ServiceBackend::Openrc, true, false, true, false).is_err());
        assert_eq!(
            select(ServiceBackend::Systemd, true, true, false, true).unwrap(),
            ServiceBackend::Systemd
        );
    }
}
