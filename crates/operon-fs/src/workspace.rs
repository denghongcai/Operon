use crate::resolve_workspace_path;
use operon_core::{RuntimeErrorKind, RuntimeResult};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Daemon-owned workspace identity. Targets are still resolved on every call;
/// only the trusted root and its descriptor are reused.
#[derive(Debug, Clone)]
pub struct WorkspaceResolver {
    configured: PathBuf,
    canonical: PathBuf,
    root: Arc<File>,
    identity: RootIdentity,
}

type RootIdentity = (u64, u64);

fn io_error(error: std::io::Error) -> (RuntimeErrorKind, String) {
    (RuntimeErrorKind::NotFound, error.to_string())
}

#[cfg(unix)]
fn open_root(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
fn open_root(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .access_mode(windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<RootIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn path_identity(path: &Path) -> std::io::Result<RootIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(file: &File) -> std::io::Result<RootIdentity> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(windows)]
fn path_identity(path: &Path) -> std::io::Result<RootIdentity> {
    file_identity(&open_root(path)?)
}

impl WorkspaceResolver {
    pub fn new(workspace: &Path) -> RuntimeResult<Self> {
        let configured = if workspace.is_absolute() {
            workspace.to_path_buf()
        } else {
            std::env::current_dir().map_err(io_error)?.join(workspace)
        };
        let canonical = configured.canonicalize().map_err(io_error)?;
        let root = Arc::new(open_root(&canonical).map_err(io_error)?);
        if !root.metadata().map_err(io_error)?.is_dir() {
            return Err((
                RuntimeErrorKind::InvalidArgument,
                "workspace root is not a directory".into(),
            ));
        }
        let identity = file_identity(&root).map_err(io_error)?;
        let resolver = Self {
            configured,
            canonical,
            root,
            identity,
        };
        #[cfg(target_os = "linux")]
        if path_identity(&resolver.operation_root()).map_err(io_error)? != identity {
            return Err((
                RuntimeErrorKind::Forbidden,
                "workspace descriptor path is unavailable".into(),
            ));
        }
        resolver.check_root()?;
        Ok(resolver)
    }

    pub fn canonical_root(&self) -> &Path {
        &self.canonical
    }

    fn check_root(&self) -> RuntimeResult<()> {
        let canonical_alias = (self.configured != self.canonical).then_some(&self.canonical);
        for path in std::iter::once(&self.configured).chain(canonical_alias) {
            if path_identity(path).ok() != Some(self.identity) {
                return Err((RuntimeErrorKind::Forbidden, "workspace root changed or became unavailable; restart the daemon after restoring configuration".into()));
            }
        }
        Ok(())
    }

    fn operation_root(&self) -> PathBuf {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            PathBuf::from(format!("/proc/self/fd/{}", self.root.as_raw_fd()))
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.canonical.clone()
        }
    }

    fn operation_path(&self, raw: &Path) -> RuntimeResult<PathBuf> {
        let relative = raw.strip_prefix(&self.canonical).map_err(|_| {
            (
                RuntimeErrorKind::Forbidden,
                "path resolves outside workspace mount".into(),
            )
        })?;
        Ok(self.operation_root().join(relative))
    }

    fn validate(&self, raw: &Path) -> RuntimeResult<()> {
        if !raw.starts_with(&self.canonical) {
            return Err((
                RuntimeErrorKind::Forbidden,
                "path resolves outside workspace mount".into(),
            ));
        }
        #[cfg(target_os = "linux")]
        crate::linux_openat2_with_root(&self.canonical, raw, &self.root)?;
        Ok(())
    }

    fn raw(&self, path: &str) -> RuntimeResult<PathBuf> {
        self.check_root()?;
        resolve_workspace_path(&self.canonical, path)
    }

    pub fn existing(&self, path: &str) -> RuntimeResult<PathBuf> {
        let raw = self.raw(path)?.canonicalize().map_err(io_error)?;
        self.validate(&raw)?;
        self.operation_path(&raw)
    }

    pub fn existing_leaf(&self, path: &str) -> RuntimeResult<PathBuf> {
        let raw = self.raw(path)?;
        self.validate_parent(&raw, false)?;
        std::fs::symlink_metadata(&raw).map_err(io_error)?;
        self.operation_path(&raw)
    }

    pub fn write(&self, path: &str) -> RuntimeResult<PathBuf> {
        self.write_with_parent(path).map(|(path, _)| path)
    }

    /// Returns whether the resolved parent already exists, allowing callers to
    /// avoid repeated create_dir_all on ordinary in-place writes.
    pub fn write_with_parent(&self, path: &str) -> RuntimeResult<(PathBuf, bool)> {
        let raw = self.raw(path)?;
        match std::fs::symlink_metadata(&raw) {
            Ok(_) => {
                let canonical = raw.canonicalize().map_err(io_error)?;
                self.validate(&canonical)?;
                Ok((self.operation_path(&canonical)?, true))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent_exists = self.validate_parent(&raw, true)?;
                Ok((self.operation_path(&raw)?, parent_exists))
            }
            Err(error) => Err(io_error(error)),
        }
    }

    pub fn create(&self, path: &str) -> RuntimeResult<PathBuf> {
        let raw = self.raw(path)?;
        self.validate_parent(&raw, true)?;
        self.operation_path(&raw)
    }

    fn validate_parent(&self, raw: &Path, ancestors: bool) -> RuntimeResult<bool> {
        let mut parent = raw.parent().ok_or_else(|| {
            (
                RuntimeErrorKind::Forbidden,
                "path has no workspace parent".into(),
            )
        })?;
        let original_parent = parent;
        if ancestors {
            while !parent.exists() {
                parent = parent.parent().ok_or_else(|| {
                    (
                        RuntimeErrorKind::Forbidden,
                        "path has no existing workspace ancestor".into(),
                    )
                })?;
            }
        }
        self.validate(&parent.canonicalize().map_err(io_error)?)?;
        Ok(parent == original_parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "opt-in path-resolution syscall/ops benchmark"]
    fn workspace_path_resolution_ops() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"DATA").unwrap();
        let resolver = WorkspaceResolver::new(dir.path()).unwrap();
        let cached = std::env::var("OPERON_RESOLVER_BENCH_MODE").as_deref() == Ok("cached");
        let started = std::time::Instant::now();
        let operations = 10000;
        for _ in 0..operations {
            let path = if cached {
                resolver.existing("/file").unwrap()
            } else {
                crate::resolve_existing_workspace_path(dir.path(), "/file").unwrap()
            };
            assert_eq!(std::fs::metadata(path).unwrap().len(), 4);
        }
        let elapsed = started.elapsed().as_secs_f64();
        println!(
            "mode={} completed_ops={} seconds={} ops_per_second={}",
            if cached { "cached" } else { "legacy" },
            operations,
            elapsed,
            operations as f64 / elapsed
        );
    }

    #[test]
    fn resolver_reuses_root_and_preserves_existing_create_and_leaf_semantics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"CONTENT").unwrap();
        let resolver = WorkspaceResolver::new(dir.path()).unwrap();
        assert_eq!(
            resolver.canonical_root(),
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read(resolver.existing("/file").unwrap()).unwrap(),
            b"CONTENT"
        );
        assert!(resolver.existing("/missing").is_err());
        for path in ["/../outside", "/dir/../../outside"] {
            assert_eq!(
                resolver.existing(path).unwrap_err().0,
                RuntimeErrorKind::Forbidden
            );
        }
        let created = resolver.create("/parent/nested/file").unwrap();
        std::fs::create_dir_all(created.parent().unwrap()).unwrap();
        std::fs::write(created, b"NEW").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("parent/nested/file")).unwrap(),
            b"NEW"
        );
        assert!(resolver.existing_leaf("/parent/nested/file").is_ok());
    }

    #[test]
    fn root_replacement_and_deletion_fail_closed() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("file"), b"OLD").unwrap();
        let resolver = WorkspaceResolver::new(&root).unwrap();
        let selected = resolver.existing("/file").unwrap();
        std::fs::rename(&root, base.path().join("old")).unwrap();
        assert_eq!(
            resolver.existing("/file").unwrap_err().0,
            RuntimeErrorKind::Forbidden
        );
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("file"), b"OTHER ROOT").unwrap();
        for result in [
            resolver.existing("/file"),
            resolver.write("/file"),
            resolver.create("/new"),
            resolver.existing_leaf("/file"),
        ] {
            assert_eq!(result.unwrap_err().0, RuntimeErrorKind::Forbidden);
        }
        #[cfg(target_os = "linux")]
        assert_eq!(std::fs::read(selected).unwrap(), b"OLD");
        #[cfg(not(target_os = "linux"))]
        let _ = selected;
    }

    #[test]
    #[cfg(unix)]
    fn cached_root_preserves_symlink_checks_and_detects_configured_alias_changes() {
        use std::os::unix::fs::symlink;
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        let outside = base.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("file"), b"SAFE").unwrap();
        std::fs::write(outside.join("secret"), b"SECRET").unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        symlink(root.join("file"), root.join("inside")).unwrap();
        let alias = base.path().join("alias");
        symlink(&root, &alias).unwrap();
        let resolver = WorkspaceResolver::new(&alias).unwrap();
        assert_eq!(
            std::fs::read(resolver.existing("/inside").unwrap()).unwrap(),
            b"SAFE"
        );
        assert!(
            std::fs::symlink_metadata(resolver.existing_leaf("/escape").unwrap())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        for result in [
            resolver.existing("/escape/secret"),
            resolver.create("/escape/new/deep"),
            resolver.write("/escape/secret"),
        ] {
            assert_eq!(result.unwrap_err().0, RuntimeErrorKind::Forbidden);
        }
        std::fs::remove_file(&alias).unwrap();
        symlink(&outside, &alias).unwrap();
        assert_eq!(
            resolver.existing("/secret").unwrap_err().0,
            RuntimeErrorKind::Forbidden
        );
    }
}
