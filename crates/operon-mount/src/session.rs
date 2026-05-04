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
    ensure_mount_point(&mount_point)?;

    let remote_fs = Arc::new(GrpcRemoteFs::connect(options.endpoint)?);
    let root = remote_fs.stat(&remote_root)?;
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
    config.n_threads = Some(4);
    let session = fuser::spawn_mount2(fs, &mount_point, &config)
        .with_context(|| format!("failed to mount {}", mount_point.display()))?;

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
