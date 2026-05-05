#![cfg(any(target_os = "linux", target_os = "macos"))]

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
    add_platform_mount_options(&mut config.mount_options, &mount_point)?;
    config.n_threads = Some(default_mount_thread_count());
    trace_mount_event("n_threads", default_mount_thread_count().to_string());
    trace_mount_event("spawn_mount2_start", mount_point.display().to_string());
    let session = fuser::spawn_mount2(fs, &mount_point, &config)
        .with_context(|| format!("failed to mount {}", mount_point.display()))?;
    trace_mount_event("spawn_mount2_ok", mount_point.display().to_string());

    Ok(MountSession { session })
}

#[cfg(target_os = "linux")]
fn default_mount_thread_count() -> usize {
    4
}

#[cfg(not(target_os = "linux"))]
fn default_mount_thread_count() -> usize {
    1
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
fn add_platform_mount_options(
    options: &mut Vec<fuser::MountOption>,
    mount_point: &Path,
) -> anyhow::Result<()> {
    let extra_options = match std::env::var("OPERON_MOUNT_MACOS_OPTIONS") {
        Ok(value) => parse_macos_extra_mount_options(&value)?,
        Err(_) => Vec::new(),
    };
    add_macos_mount_options(
        options,
        mount_point,
        macos_mount_backend().as_deref(),
        &extra_options,
    )?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn add_platform_mount_options(
    _options: &mut [fuser::MountOption],
    _mount_point: &Path,
) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_mount_backend() -> Option<String> {
    match std::env::var("OPERON_MOUNT_MACOS_BACKEND") {
        Ok(value) if value.is_empty() => return None,
        Ok(value) => return Some(value),
        Err(_) => {}
    }

    Some("nfs".to_string())
}

#[cfg(any(target_os = "macos", test))]
fn add_macos_mount_options(
    options: &mut Vec<fuser::MountOption>,
    mount_point: &Path,
    backend: Option<&str>,
    extra_options: &[String],
) -> anyhow::Result<()> {
    if let Some(backend) = backend {
        if backend.eq_ignore_ascii_case("kernel") {
            anyhow::bail!(
                "macOS kernel FUSE backend is not supported; install FUSE-T and use OPERON_MOUNT_MACOS_BACKEND=nfs, smb, or fskit"
            );
        }
        if backend.eq_ignore_ascii_case("fskit") && !mount_point.starts_with("/Volumes") {
            anyhow::bail!(
                "macOS FUSE FSKit backend requires a mount point under /Volumes; choose /Volumes/<name> or set OPERON_MOUNT_MACOS_BACKEND=nfs to use the FUSE-T NFS backend"
            );
        }
        trace_mount_event("macos_backend", backend);
        options.push(fuser::MountOption::CUSTOM(format!("backend={backend}")));
    }

    for option in extra_options {
        trace_mount_event("macos_option", option);
        options.push(fuser::MountOption::CUSTOM(option.clone()));
    }

    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_extra_mount_options(value: &str) -> anyhow::Result<Vec<String>> {
    let mut options = Vec::new();
    for option in value
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
    {
        if option.starts_with('-') {
            anyhow::bail!(
                "macOS FUSE-T extra mount options must be -o options such as nobrowse,noattrcache,rwsize=65536; raw arguments like -d or -l are not supported through OPERON_MOUNT_MACOS_OPTIONS"
            );
        }
        if option.starts_with("backend=") {
            anyhow::bail!(
                "set macOS FUSE-T backend with OPERON_MOUNT_MACOS_BACKEND instead of OPERON_MOUNT_MACOS_OPTIONS"
            );
        }
        if option.contains('\n') || option.contains('\r') {
            anyhow::bail!("macOS FUSE-T extra mount options must not contain newlines");
        }
        options.push(option.to_string());
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_mount_options_include_backend_and_extra_fuse_t_options() {
        let mut options = Vec::new();

        add_macos_mount_options(
            &mut options,
            Path::new("/Volumes/operon-test"),
            Some("nfs"),
            &["nobrowse".to_string(), "noattrcache".to_string()],
        )
        .expect("macos options");

        assert_eq!(
            options,
            vec![
                fuser::MountOption::CUSTOM("backend=nfs".to_string()),
                fuser::MountOption::CUSTOM("nobrowse".to_string()),
                fuser::MountOption::CUSTOM("noattrcache".to_string()),
            ]
        );
    }

    #[test]
    fn macos_extra_options_reject_raw_fuse_t_arguments() {
        let error = parse_macos_extra_mount_options("-d,nobrowse").expect_err("raw args rejected");

        assert!(
            error
                .to_string()
                .contains("macOS FUSE-T extra mount options must be -o options"),
            "{error}"
        );
    }

    #[test]
    fn default_mount_thread_count_matches_fuser_platform_support() {
        if cfg!(target_os = "linux") {
            assert_eq!(default_mount_thread_count(), 4);
        } else {
            assert_eq!(default_mount_thread_count(), 1);
        }
    }
}
