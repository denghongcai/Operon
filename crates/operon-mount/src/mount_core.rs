use std::{ffi::OsStr, sync::Arc};

use operon_core::{FsList, FsStat};

/// Platform-neutral filesystem operations needed by live mount adapters.
pub trait RemoteFs: Send + Sync {
    fn stat(&self, path: &str) -> anyhow::Result<FsStat>;
    fn list(&self, path: &str) -> anyhow::Result<FsList>;
    fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>>;
    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64>;
    fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat>;
    fn mkdir(&self, path: &str) -> anyhow::Result<FsStat>;
    fn delete(&self, path: &str) -> anyhow::Result<()>;
    fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct MountAdapterCore {
    remote_fs: Arc<dyn RemoteFs>,
}

#[derive(Debug, Clone)]
pub struct MountDirectoryEntry {
    pub name: String,
    pub stat: FsStat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountErrorKind {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    InvalidInput,
    FailedPrecondition,
    Unknown,
}

impl MountAdapterCore {
    pub fn new(remote_fs: Arc<dyn RemoteFs>) -> Self {
        Self { remote_fs }
    }

    pub fn stat(&self, path: &str) -> anyhow::Result<FsStat> {
        self.remote_fs.stat(&normalize_remote_path(path)?)
    }

    pub fn list_dir(&self, path: &str) -> anyhow::Result<Vec<MountDirectoryEntry>> {
        let list = self.remote_fs.list(&normalize_remote_path(path)?)?;
        Ok(list
            .entries
            .into_iter()
            .map(|entry| MountDirectoryEntry {
                name: entry.name,
                stat: FsStat {
                    path: entry.path,
                    is_file: entry.is_file,
                    is_dir: entry.is_dir,
                    size: entry.size,
                    version: entry.version,
                },
            })
            .collect())
    }

    pub fn read_file(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>> {
        self.remote_fs
            .read_range(&normalize_remote_path(path)?, offset, size)
    }

    pub fn write_file(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        self.remote_fs
            .write_range(&normalize_remote_path(path)?, offset, data)
    }

    pub fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat> {
        self.remote_fs.truncate(&normalize_remote_path(path)?, size)
    }

    pub fn mkdir_child(&self, parent: &str, child: &str) -> anyhow::Result<FsStat> {
        self.mkdir(&join_remote_child(parent, child)?)
    }

    pub fn create_file_child(&self, parent: &str, child: &str) -> anyhow::Result<FsStat> {
        self.create_file(&join_remote_child(parent, child)?)
    }

    pub fn delete_child(&self, parent: &str, child: &str) -> anyhow::Result<()> {
        self.delete(&join_remote_child(parent, child)?)
    }

    pub fn rename_child(
        &self,
        parent: &str,
        child: &str,
        new_parent: &str,
        new_child: &str,
    ) -> anyhow::Result<()> {
        self.rename(
            &join_remote_child(parent, child)?,
            &join_remote_child(new_parent, new_child)?,
        )
    }

    pub fn mkdir(&self, path: &str) -> anyhow::Result<FsStat> {
        self.remote_fs.mkdir(&normalize_remote_path(path)?)
    }

    pub fn create_file(&self, path: &str) -> anyhow::Result<FsStat> {
        let path = normalize_remote_path(path)?;
        self.remote_fs.write_range(&path, 0, &[])?;
        self.remote_fs.stat(&path)
    }

    pub fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.remote_fs.delete(&normalize_remote_path(path)?)
    }

    pub fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()> {
        self.remote_fs.rename(
            &normalize_remote_path(from_path)?,
            &normalize_remote_path(to_path)?,
        )
    }
}

pub fn classify_mount_error(error: &anyhow::Error) -> MountErrorKind {
    if let Some(status) = error.downcast_ref::<tonic::Status>() {
        match status.code() {
            tonic::Code::NotFound => return MountErrorKind::NotFound,
            tonic::Code::AlreadyExists => return MountErrorKind::AlreadyExists,
            tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => {
                return MountErrorKind::PermissionDenied;
            }
            tonic::Code::InvalidArgument => return MountErrorKind::InvalidInput,
            tonic::Code::FailedPrecondition => return MountErrorKind::FailedPrecondition,
            _ => {}
        }
    }

    MountErrorKind::Unknown
}

pub fn normalize_remote_path(path: &str) -> anyhow::Result<String> {
    if !path.starts_with('/') {
        anyhow::bail!("remote mount path must be absolute");
    }
    if path.as_bytes().contains(&0) {
        anyhow::bail!("remote mount path cannot contain NUL");
    }

    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => anyhow::bail!("remote mount path cannot contain `..`"),
            value => parts.push(value),
        }
    }

    if parts.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", parts.join("/")))
    }
}

pub fn validate_child_name(name: &OsStr) -> anyhow::Result<&str> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path names must be valid UTF-8"))?;
    validate_child_path_segment(name)?;
    Ok(name)
}

pub fn join_remote_child(parent: &str, child: &str) -> anyhow::Result<String> {
    let parent = normalize_remote_path(parent)?;
    validate_child_path_segment(child)?;
    if parent == "/" {
        Ok(format!("/{child}"))
    } else {
        Ok(format!("{parent}/{child}"))
    }
}

fn validate_child_path_segment(child: &str) -> anyhow::Result<()> {
    if child.is_empty()
        || child == "."
        || child == ".."
        || child.contains('/')
        || child.as_bytes().contains(&0)
    {
        anyhow::bail!("invalid child path name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    #[test]
    fn normalizes_remote_root_paths() {
        assert_eq!(normalize_remote_path("/").expect("root"), "/");
        assert_eq!(
            normalize_remote_path("/workspace//project/").expect("path"),
            "/workspace/project"
        );
    }

    #[test]
    fn rejects_mount_root_escape_paths() {
        let error = normalize_remote_path("/workspace/../secret").expect_err("escape");

        assert!(error.to_string().contains("cannot contain `..`"));
    }

    #[test]
    fn joins_remote_child_under_normalized_parent() {
        assert_eq!(
            join_remote_child("/workspace//project/", "file.txt").expect("child path"),
            "/workspace/project/file.txt"
        );
    }

    #[test]
    fn rejects_invalid_child_segments() {
        for child in ["", ".", "..", "nested/file", "nul\0byte"] {
            let error = join_remote_child("/workspace", child).expect_err("invalid child");
            assert!(error.to_string().contains("invalid child path name"));
        }
    }

    #[test]
    fn core_normalizes_paths_before_dispatching_remote_operations() {
        let remote = Arc::new(MockRemoteFs::new());
        remote.insert_stat(file_stat("/workspace/file.txt", 8));
        let core = MountAdapterCore::new(remote.clone());

        assert_eq!(
            core.stat("/workspace//file.txt/")
                .expect("normalized stat")
                .path,
            "/workspace/file.txt"
        );
        core.read_file("/workspace//file.txt/", 4, 32)
            .expect("normalized read");
        core.write_file("/workspace//file.txt/", 4, b"hello")
            .expect("normalized write");
        core.truncate("/workspace//file.txt/", 3)
            .expect("normalized truncate");

        assert_eq!(
            remote.calls(),
            vec![
                "stat:/workspace/file.txt",
                "read:/workspace/file.txt:4:32",
                "write:/workspace/file.txt:4:5",
                "truncate:/workspace/file.txt:3",
            ]
        );
    }

    #[test]
    fn core_maps_child_operations_to_valid_remote_paths() {
        let remote = Arc::new(MockRemoteFs::new());
        remote.insert_stat(dir_stat("/workspace/new-dir"));
        remote.insert_stat(file_stat("/workspace/new-file.txt", 0));
        let core = MountAdapterCore::new(remote.clone());

        core.mkdir_child("/workspace//", "new-dir")
            .expect("mkdir child");
        core.create_file_child("/workspace//", "new-file.txt")
            .expect("create file child");
        core.delete_child("/workspace//", "old.txt")
            .expect("delete child");
        core.rename_child(
            "/workspace//",
            "new-file.txt",
            "/workspace/archive",
            "moved.txt",
        )
        .expect("rename child");

        assert_eq!(
            remote.calls(),
            vec![
                "mkdir:/workspace/new-dir",
                "write:/workspace/new-file.txt:0:0",
                "stat:/workspace/new-file.txt",
                "delete:/workspace/old.txt",
                "rename:/workspace/new-file.txt:/workspace/archive/moved.txt",
            ]
        );
    }

    #[test]
    fn core_converts_list_entries_to_stats() {
        let remote = Arc::new(MockRemoteFs::new());
        remote.insert_list(FsList {
            path: "/workspace".to_string(),
            next_page_token: String::new(),
            entries: vec![
                operon_core::FsEntry {
                    name: "dir".to_string(),
                    path: "/workspace/dir".to_string(),
                    is_file: false,
                    is_dir: true,
                    size: 0,
                    version: "dir-v".to_string(),
                },
                operon_core::FsEntry {
                    name: "file.txt".to_string(),
                    path: "/workspace/file.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 42,
                    version: "file-v".to_string(),
                },
            ],
        });
        let core = MountAdapterCore::new(remote);

        let entries = core.list_dir("/workspace//").expect("list dir");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "dir");
        assert_eq!(entries[0].stat.path, "/workspace/dir");
        assert!(!entries[0].stat.is_file);
        assert!(entries[0].stat.is_dir);
        assert_eq!(entries[0].stat.size, 0);
        assert_eq!(entries[0].stat.version, "dir-v");
        assert_eq!(entries[1].name, "file.txt");
        assert_eq!(entries[1].stat.size, 42);
    }

    #[test]
    fn classifies_remote_errors_without_platform_errno() {
        assert_eq!(
            classify_mount_error(&tonic::Status::not_found("missing").into()),
            MountErrorKind::NotFound
        );
        assert_eq!(
            classify_mount_error(&tonic::Status::already_exists("exists").into()),
            MountErrorKind::AlreadyExists
        );
        assert_eq!(
            classify_mount_error(&tonic::Status::permission_denied("denied").into()),
            MountErrorKind::PermissionDenied
        );
        assert_eq!(
            classify_mount_error(&tonic::Status::unauthenticated("missing token").into()),
            MountErrorKind::PermissionDenied
        );
        assert_eq!(
            classify_mount_error(&tonic::Status::invalid_argument("bad path").into()),
            MountErrorKind::InvalidInput
        );
        assert_eq!(
            classify_mount_error(&tonic::Status::failed_precondition("version").into()),
            MountErrorKind::FailedPrecondition
        );
        assert_eq!(
            classify_mount_error(&anyhow::anyhow!("plain error")),
            MountErrorKind::Unknown
        );
    }

    struct MockRemoteFs {
        stats: Mutex<BTreeMap<String, FsStat>>,
        lists: Mutex<BTreeMap<String, FsList>>,
        calls: Mutex<Vec<String>>,
    }

    impl MockRemoteFs {
        fn new() -> Self {
            Self {
                stats: Mutex::new(BTreeMap::new()),
                lists: Mutex::new(BTreeMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn insert_stat(&self, stat: FsStat) {
            self.stats
                .lock()
                .expect("stats")
                .insert(stat.path.clone(), stat);
        }

        fn insert_list(&self, list: FsList) {
            self.lists
                .lock()
                .expect("lists")
                .insert(list.path.clone(), list);
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl RemoteFs for MockRemoteFs {
        fn stat(&self, path: &str) -> anyhow::Result<FsStat> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("stat:{path}"));
            self.stats
                .lock()
                .expect("stats")
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing stat for {path}"))
        }

        fn list(&self, path: &str) -> anyhow::Result<FsList> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("list:{path}"));
            self.lists
                .lock()
                .expect("lists")
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing list for {path}"))
        }

        fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("read:{path}:{offset}:{size}"));
            Ok(vec![0; size as usize])
        }

        fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("write:{path}:{offset}:{}", data.len()));
            Ok(data.len() as u64)
        }

        fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("truncate:{path}:{size}"));
            let stat = file_stat(path, size);
            self.insert_stat(stat.clone());
            Ok(stat)
        }

        fn mkdir(&self, path: &str) -> anyhow::Result<FsStat> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("mkdir:{path}"));
            self.stats
                .lock()
                .expect("stats")
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing mkdir stat for {path}"))
        }

        fn delete(&self, path: &str) -> anyhow::Result<()> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("delete:{path}"));
            Ok(())
        }

        fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("rename:{from_path}:{to_path}"));
            Ok(())
        }
    }

    fn dir_stat(path: &str) -> FsStat {
        FsStat {
            path: path.to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
            version: "dir-version".to_string(),
        }
    }

    fn file_stat(path: &str, size: u64) -> FsStat {
        FsStat {
            path: path.to_string(),
            is_file: true,
            is_dir: false,
            size,
            version: "file-version".to_string(),
        }
    }
}
