#![cfg(any(target_os = "linux", target_os = "macos"))]

#[cfg(target_os = "macos")]
use std::process::Command;
use std::{
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    time::Duration,
};

use anyhow::Context;
use operon_network::NodeEndpoint;

use crate::{
    fuse_fs::OperonFuseFs,
    mount_core::{normalize_remote_path, RemoteFs},
    remote_client::GrpcRemoteFs,
};

fn trace_mount_event(event: impl AsRef<str>, detail: impl AsRef<str>) {
    if std::env::var_os("OPERON_MOUNT_TRACE").is_some() {
        eprintln!("operon-mount unix {}: {}", event.as_ref(), detail.as_ref());
    }
}

#[derive(Debug, Clone)]
pub struct MountOptions {
    pub endpoint: NodeEndpoint,
    pub remote_path: String,
    pub mount_point: PathBuf,
}

pub struct MountSession {
    session: fuser::BackgroundSession,
}

impl MountSession {
    pub fn wait_for_shutdown(self) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel();
        let handler_tx = tx.clone();
        let _tx_guard = match ctrlc::set_handler(move || {
            let _ = handler_tx.send(());
        }) {
            Ok(()) => None,
            Err(error) => {
                eprintln!(
                    "warning: failed to install mount shutdown handler; terminate the mount process to unmount: {error}"
                );
                Some(tx)
            }
        };

        loop {
            match rx.recv_timeout(Duration::from_secs(3600)) {
                Ok(()) => return self.unmount(),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => continue,
            }
        }
    }

    pub fn unmount(self) -> anyhow::Result<()> {
        self.session.umount_and_join()?;
        Ok(())
    }
}

pub fn spawn_mount(options: MountOptions) -> anyhow::Result<MountSession> {
    let remote_root = normalize_remote_path(&options.remote_path)?;
    let mount_point = options.mount_point;
    trace_mount_event("start", mount_point.display().to_string());
    ensure_mount_point(&mount_point)?;
    trace_mount_event("mount_point_ready", mount_point.display().to_string());

    trace_mount_event("remote_connect_start", remote_root.clone());
    let remote_fs = Arc::new(GrpcRemoteFs::connect(options.endpoint)?);
    trace_mount_event("remote_connect_ok", remote_root.clone());
    let root = remote_fs.stat(&remote_root)?;
    trace_mount_event("remote_root_stat", root.path.clone());
    if !root.is_dir {
        anyhow::bail!("mount root `{remote_root}` is not a directory");
    }

    let fs = OperonFuseFs::new(remote_fs, root);
    let mut config = fuser::Config::default();
    config.mount_options = vec![
        fuser::MountOption::FSName("operon".to_string()),
        fuser::MountOption::Subtype("operon".to_string()),
        fuser::MountOption::NoDev,
        fuser::MountOption::NoSuid,
        fuser::MountOption::NoExec,
    ];
    add_platform_mount_options(&mut config.mount_options);
    config.n_threads = Some(4);
    trace_mount_event("spawn_mount2_start", mount_point.display().to_string());
    let session = fuser::spawn_mount2(fs, &mount_point, &config)
        .with_context(|| format!("failed to mount {}", mount_point.display()))?;
    trace_mount_event("spawn_mount2_ok", mount_point.display().to_string());

    Ok(MountSession { session })
}

fn ensure_mount_point(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create mount point {}", path.display()))?;
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("failed to stat mount point {}", path.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("mount point `{}` is not a directory", path.display());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn add_platform_mount_options(options: &mut Vec<fuser::MountOption>) {
    if let Some(backend) = macos_mount_backend() {
        trace_mount_event("macos_backend", backend.clone());
        options.push(fuser::MountOption::CUSTOM(format!("backend={backend}")));
    }
}

#[cfg(not(target_os = "macos"))]
fn add_platform_mount_options(_options: &mut Vec<fuser::MountOption>) {}

#[cfg(target_os = "macos")]
fn macos_mount_backend() -> Option<String> {
    match std::env::var("OPERON_MOUNT_MACOS_BACKEND") {
        Ok(value) if value.eq_ignore_ascii_case("kernel") || value.is_empty() => return None,
        Ok(value) => return Some(value),
        Err(_) => {}
    }

    let version = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())?;
    if macos_supports_fskit(version.trim()) {
        Some("fskit".to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_supports_fskit(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    major > 15 || (major == 15 && minor >= 4)
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::macos_supports_fskit;

    #[test]
    fn macos_fskit_support_starts_at_15_4() {
        assert!(!macos_supports_fskit("14.7.1"));
        assert!(!macos_supports_fskit("15.3.9"));
        assert!(macos_supports_fskit("15.4"));
        assert!(macos_supports_fskit("15.7.4"));
        assert!(macos_supports_fskit("26.0"));
    }
}
