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
use operon_core::runtime::NodeEndpoint;

use crate::{
    fuse_fs::OperonFuseFs,
    mount_core::{normalize_remote_path, RemoteFs},
    remote_client::GrpcRemoteFs,
};

fn trace_mount_event(event: impl AsRef<str>, detail: impl AsRef<str>) {
    if crate::trace_enabled() {
        eprintln!("operon-mount unix {}: {}", event.as_ref(), detail.as_ref());
    }
}

#[derive(Debug, Clone)]
pub struct MountOptions {
    pub endpoint: NodeEndpoint,
    pub remote_path: String,
    pub mount_point: PathBuf,
    pub reads: operon_core::runtime::MountReadConfig,
}

pub struct MountSession {
    session: Option<fuser::BackgroundSession>,
    remote: Arc<GrpcRemoteFs>,
    #[cfg(target_os = "linux")]
    identity: (PathBuf, u64),
    #[cfg(target_os = "linux")]
    unmount_poll: Duration,
}

impl MountSession {
    pub fn wait_for_shutdown(self) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        let mut self_session = self;
        #[cfg(not(target_os = "linux"))]
        let self_session = self;
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
            #[cfg(target_os = "linux")]
            if self_session
                .session
                .as_ref()
                .is_some_and(|session| session.guard.is_finished())
            {
                return self_session.finish(true);
            }
            #[cfg(target_os = "linux")]
            if linux_mount_id(&self_session.identity.0)? != Some(self_session.identity.1) {
                return self_session.unmount();
            }
            #[cfg(target_os = "linux")]
            let interval = self_session.unmount_poll;
            #[cfg(not(target_os = "linux"))]
            let interval = Duration::from_secs(3600);
            match rx.recv_timeout(interval) {
                Ok(()) => return self_session.unmount(),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => continue,
            }
        }
    }

    pub fn unmount(mut self) -> anyhow::Result<()> {
        self.finish(false)
    }

    fn finish(&mut self, detached: bool) -> anyhow::Result<()> {
        self.remote.shutdown_reads();
        let Some(session) = self.session.take() else {
            return Ok(());
        };
        #[cfg(target_os = "linux")]
        {
            if detached {
                session.detached_and_join()?;
                return Ok(());
            }
            match linux_mount_id(&self.identity.0) {
                Ok(identity) if identity != Some(self.identity.1) => {
                    session.detached_and_join()?;
                    return Ok(());
                }
                Err(error) => {
                    // Unknown ownership must not cause a path-based unmount.
                    // Still disconnect and join, and report the verification error.
                    session.detached_and_join()?;
                    return Err(error.into());
                }
                Ok(_) => {}
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = detached;
        session.umount_and_join()?;
        Ok(())
    }
}

impl Drop for MountSession {
    fn drop(&mut self) {
        if self.session.is_some() {
            if let Err(error) = self.finish(false) {
                eprintln!("warning: mount session cleanup failed: {error:#}");
            }
        }
    }
}

pub fn spawn_mount(options: MountOptions) -> anyhow::Result<MountSession> {
    options.reads.validate().map_err(anyhow::Error::msg)?;
    #[cfg(target_os = "linux")]
    let unmount_poll = Duration::from_millis(options.reads.unmount_poll_ms);
    let async_reads = match options.reads.mode {
        operon_core::runtime::MountReadMode::Auto => cfg!(target_os = "linux"),
        operon_core::runtime::MountReadMode::Sync => false,
        operon_core::runtime::MountReadMode::Async => true,
    };
    let pending_reads = options.reads.max_pending_reads;
    #[cfg(not(target_os = "linux"))]
    if options.reads.worker_threads.is_some() {
        anyhow::bail!("mount worker_threads is supported only on Linux FUSE");
    }
    let workers = options
        .reads
        .worker_threads
        .unwrap_or_else(default_mount_thread_count);
    let remote_root = normalize_remote_path(&options.remote_path)?;
    let mount_point = options.mount_point;
    trace_mount_event("start", mount_point.display().to_string());
    ensure_mount_point(&mount_point)?;
    #[cfg(target_os = "linux")]
    let mount_point = mount_point.canonicalize()?;
    trace_mount_event("mount_point_ready", mount_point.display().to_string());

    trace_mount_event("remote_connect_start", remote_root.clone());
    let remote_fs = Arc::new(GrpcRemoteFs::connect_with_reads(
        options.endpoint,
        options.reads,
    )?);
    trace_mount_event("remote_connect_ok", remote_root.clone());
    let root = remote_fs.stat(&remote_root)?;
    trace_mount_event("remote_root_stat", root.path.clone());
    if !root.is_dir {
        anyhow::bail!("mount root `{remote_root}` is not a directory");
    }

    let mut fs = OperonFuseFs::new(remote_fs.clone(), root);
    fs.reads = Some(crate::read_scheduler::ReadScheduler::new(
        remote_fs.clone(),
        pending_reads,
        async_reads,
    ));
    let mut config = fuser::Config::default();
    config.mount_options = base_mount_options();
    add_platform_mount_options(&mut config.mount_options, &mount_point)?;
    config.n_threads = Some(workers);
    trace_mount_event("n_threads", workers.to_string());
    trace_mount_event("spawn_mount2_start", mount_point.display().to_string());
    let session = fuser::spawn_mount2(fs, &mount_point, &config)
        .with_context(|| format!("failed to mount {}", mount_point.display()))?;
    trace_mount_event("spawn_mount2_ok", mount_point.display().to_string());

    #[cfg(target_os = "linux")]
    let identity = match session.mount_id() {
        Some(id) => (mount_point, id),
        None => {
            remote_fs.shutdown_reads();
            session.detached_and_join()?;
            anyhow::bail!("mount was externally detached during startup");
        }
    };
    Ok(MountSession {
        session: Some(session),
        remote: remote_fs,
        #[cfg(target_os = "linux")]
        identity,
        #[cfg(target_os = "linux")]
        unmount_poll,
    })
}

/// Read kernel mount bookkeeping, never metadata from the possibly hung FUSE
/// filesystem. Mount IDs distinguish the original mount from a replacement.
#[cfg(target_os = "linux")]
fn linux_mount_id(path: &Path) -> std::io::Result<Option<u64>> {
    Ok(mount_id_from_info(
        &std::fs::read("/proc/self/mountinfo")?,
        path,
    ))
}

#[cfg(target_os = "linux")]
fn mount_id_from_info(info: &[u8], path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    info.split(|byte| *byte == b'\n')
        .rev()
        .find_map(|line| {
            let fields: Vec<_> = line.split(|byte| *byte == b' ').collect();
            if fields.len() < 6 {
                return None;
            }
            let encoded = fields[4];
            let mut decoded = Vec::with_capacity(encoded.len());
            let mut index = 0;
            while index < encoded.len() {
                if encoded[index] == b'\\'
                    && index + 3 < encoded.len()
                    && encoded[index + 1..index + 4]
                        .iter()
                        .all(|byte| (b'0'..=b'7').contains(byte))
                {
                    let octal = &encoded[index + 1..index + 4];
                    let value = u16::from(octal[0] - b'0') * 64
                        + u16::from(octal[1] - b'0') * 8
                        + u16::from(octal[2] - b'0');
                    decoded.push(u8::try_from(value).ok()?);
                    index += 4;
                } else {
                    decoded.push(encoded[index]);
                    index += 1;
                }
            }
            (decoded == path.as_os_str().as_bytes())
                .then(|| {
                    let id: u64 = std::str::from_utf8(fields[0]).ok()?.parse().ok()?;
                    let separator = fields.iter().position(|field| *field == b"-")?;
                    let fs_type = fields.get(separator + 1).copied()?;
                    let owned = (fs_type == b"fuse.operon" || fs_type == b"fuse")
                        && fields.get(separator + 2).copied() == Some(b"operon".as_slice());
                    Some((id, owned))
                })
                .flatten()
        })
        .and_then(|(id, owned)| owned.then_some(id))
}

#[cfg(target_os = "macos")]
fn base_mount_options() -> Vec<fuser::MountOption> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
fn base_mount_options() -> Vec<fuser::MountOption> {
    vec![
        fuser::MountOption::FSName("operon".to_string()),
        fuser::MountOption::Subtype("operon".to_string()),
        fuser::MountOption::NoDev,
        fuser::MountOption::NoSuid,
        fuser::MountOption::NoExec,
    ]
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
    #[cfg(target_os = "linux")]
    #[test]
    fn mount_identity_handles_detachment_replacement_and_escaped_paths() {
        use super::mount_id_from_info;
        use std::path::Path;
        let original = b"7 1 0:7 / /mnt/remote rw - fuse.operon operon rw\n";
        assert_eq!(
            mount_id_from_info(original, Path::new("/mnt/remote")),
            Some(7)
        );
        assert_eq!(
            mount_id_from_info(
                b"7 1 0:7 / /mnt/remote rw - fuse operon rw\n",
                Path::new("/mnt/remote")
            ),
            Some(7)
        );
        assert_eq!(
            mount_id_from_info(b"1 0 0:1 / / rw - ext4 root rw\n", Path::new("/mnt/remote")),
            None
        );
        let replacement = b"7 1 0:7 / /mnt/remote rw - fuse.operon operon rw\n9 1 0:9 / /mnt/remote rw - tmpfs tmpfs rw\n";
        assert_eq!(
            mount_id_from_info(replacement, Path::new("/mnt/remote")),
            None
        );
        assert_eq!(
            mount_id_from_info(
                b"12 1 0:12 / /mnt/a\\040b\\134c rw - fuse.operon operon rw\n",
                Path::new("/mnt/a b\\c")
            ),
            Some(12)
        );
        assert_eq!(
            mount_id_from_info(b"malformed\n", Path::new("/mnt/remote")),
            None
        );
    }
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

    #[test]
    fn base_mount_options_match_platform_mount_backend() {
        if cfg!(target_os = "macos") {
            assert!(base_mount_options().is_empty());
        } else {
            assert_eq!(
                base_mount_options(),
                vec![
                    fuser::MountOption::FSName("operon".to_string()),
                    fuser::MountOption::Subtype("operon".to_string()),
                    fuser::MountOption::NoDev,
                    fuser::MountOption::NoSuid,
                    fuser::MountOption::NoExec,
                ]
            );
        }
    }
}
