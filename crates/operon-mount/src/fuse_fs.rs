#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{
    ffi::OsStr,
    path::Path,
    sync::{Arc, RwLock, RwLockWriteGuard},
    time::Duration,
};

use operon_core::FsStat;

use crate::{
    errors::errno_for_error,
    fuse_attr::{attr_trace_detail, file_attr},
    fuse_semantics::{rename_flags_errno, xattr_decision, XattrDecision, XattrRequest},
    inode_table::{InodeEntry, InodeTable},
    mount_core::{MountAdapterCore, RemoteFs},
    path::{join_remote_child, validate_child_name},
};

const TTL: Duration = Duration::from_secs(1);

fn trace_fuse_event(event: impl AsRef<str>, detail: impl AsRef<str>) {
    if std::env::var_os("OPERON_MOUNT_TRACE").is_some() {
        eprintln!("operon-mount fuse {}: {}", event.as_ref(), detail.as_ref());
    }
}

pub(crate) struct OperonFuseFs {
    core: MountAdapterCore,
    inodes: RwLock<InodeTable>,
}

impl OperonFuseFs {
    pub(crate) fn new(remote_fs: Arc<dyn RemoteFs>, root: FsStat) -> Self {
        Self {
            core: MountAdapterCore::new(remote_fs),
            inodes: RwLock::new(InodeTable::new(root)),
        }
    }

    fn inode(&self, ino: fuser::INodeNo) -> Option<InodeEntry> {
        self.inodes.read().ok()?.get(ino)
    }

    fn write_inodes(&self) -> anyhow::Result<RwLockWriteGuard<'_, InodeTable>> {
        self.inodes
            .write()
            .map_err(|_| anyhow::anyhow!("inode table poisoned"))
    }

    fn lookup_child(&self, parent: fuser::INodeNo, name: &OsStr) -> anyhow::Result<InodeEntry> {
        let parent_entry = self
            .inode(parent)
            .ok_or_else(|| anyhow::anyhow!("parent inode not found"))?;
        if !parent_entry.is_dir {
            anyhow::bail!("parent is not a directory");
        }
        let name = validate_child_name(name)?;
        let path = join_remote_child(&parent_entry.path, name)?;
        let stat = self.core.stat(&path)?;
        self.write_inodes()?.upsert(parent, name.to_string(), stat)
    }

    fn child_path(&self, parent: fuser::INodeNo, name: &OsStr) -> anyhow::Result<String> {
        let parent_entry = self
            .inode(parent)
            .ok_or_else(|| anyhow::anyhow!("parent inode not found"))?;
        if !parent_entry.is_dir {
            anyhow::bail!("parent is not a directory");
        }
        let name = validate_child_name(name)?;
        join_remote_child(&parent_entry.path, name)
    }

    fn upsert_child(
        &self,
        parent: fuser::INodeNo,
        name: &OsStr,
        stat: FsStat,
    ) -> anyhow::Result<InodeEntry> {
        let name = validate_child_name(name)?;
        self.write_inodes()?.upsert(parent, name.to_string(), stat)
    }

    fn refresh_inode_stat(&self, entry: InodeEntry, stat: FsStat) -> anyhow::Result<InodeEntry> {
        self.write_inodes()
            .and_then(|mut table| table.upsert(entry.parent, entry.name, stat))
    }

    fn access_errno(&self, ino: fuser::INodeNo) -> Option<fuser::Errno> {
        self.inode(ino).map_or(Some(fuser::Errno::ENOENT), |_| None)
    }
}

impl fuser::Filesystem for OperonFuseFs {
    fn init(
        &mut self,
        _req: &fuser::Request,
        _config: &mut fuser::KernelConfig,
    ) -> std::io::Result<()> {
        trace_fuse_event("init", "ok");
        Ok(())
    }

    fn destroy(&mut self) {
        trace_fuse_event("destroy", "ok");
    }

    fn lookup(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        reply: fuser::ReplyEntry,
    ) {
        trace_fuse_event("lookup", format!("parent={parent:?} name={name:?}"));
        match self.lookup_child(parent, name) {
            Ok(entry) => {
                let attr = file_attr(&entry);
                trace_fuse_event("lookup_attr", attr_trace_detail(&attr));
                reply.entry(&TTL, &attr, fuser::Generation(0));
            }
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn getattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: Option<fuser::FileHandle>,
        reply: fuser::ReplyAttr,
    ) {
        trace_fuse_event("getattr", format!("ino={ino:?}"));
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };

        match self.core.stat(&entry.path) {
            Ok(stat) => match self.refresh_inode_stat(entry, stat) {
                Ok(entry) => {
                    let attr = file_attr(&entry);
                    trace_fuse_event("getattr_attr", attr_trace_detail(&attr));
                    reply.attr(&TTL, &attr);
                }
                Err(error) => reply.error(errno_for_error(&error)),
            },
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn open(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _flags: fuser::OpenFlags,
        reply: fuser::ReplyOpen,
    ) {
        trace_fuse_event("open", format!("ino={ino:?}"));
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if entry.is_dir {
            reply.error(fuser::Errno::EISDIR);
            return;
        }
        reply.opened(
            fuser::FileHandle(u64::from(ino)),
            fuser::FopenFlags::empty(),
        );
    }

    fn setattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<fuser::FileHandle>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<fuser::BsdFileFlags>,
        reply: fuser::ReplyAttr,
    ) {
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };

        if let Some(size) = size {
            match self
                .core
                .truncate(&entry.path, size)
                .and_then(|stat| self.refresh_inode_stat(entry, stat))
            {
                Ok(entry) => reply.attr(&TTL, &file_attr(&entry)),
                Err(error) => reply.error(errno_for_error(&error)),
            }
            return;
        }

        reply.attr(&TTL, &file_attr(&entry));
    }

    fn mknod(
        &self,
        _req: &fuser::Request,
        _parent: fuser::INodeNo,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        reply.error(fuser::Errno::ENOSYS);
    }

    fn mkdir(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        let path = match self.child_path(parent, name) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        match self
            .core
            .mkdir(&path)
            .and_then(|stat| self.upsert_child(parent, name, stat))
        {
            Ok(entry) => reply.entry(&TTL, &file_attr(&entry), fuser::Generation(0)),
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn unlink(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let path = match self.child_path(parent, name) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        match self.core.delete(&path) {
            Ok(()) => match self.write_inodes() {
                Ok(mut table) => {
                    table.remove_subtree(&path);
                    reply.ok();
                }
                Err(error) => reply.error(errno_for_error(&error)),
            },
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn rmdir(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let path = match self.child_path(parent, name) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        match self.core.delete(&path) {
            Ok(()) => match self.write_inodes() {
                Ok(mut table) => {
                    table.remove_subtree(&path);
                    reply.ok();
                }
                Err(error) => reply.error(errno_for_error(&error)),
            },
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn symlink(
        &self,
        _req: &fuser::Request,
        _parent: fuser::INodeNo,
        _link_name: &OsStr,
        _target: &Path,
        reply: fuser::ReplyEntry,
    ) {
        reply.error(fuser::Errno::ENOSYS);
    }

    fn rename(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        newparent: fuser::INodeNo,
        newname: &OsStr,
        flags: fuser::RenameFlags,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event(
            "rename",
            format!("parent={parent:?} name={name:?} newparent={newparent:?} newname={newname:?} flags={flags:?}"),
        );
        if let Some(errno) = rename_flags_errno(flags) {
            trace_fuse_event("rename_error", format!("unsupported flags={flags:?}"));
            reply.error(errno);
            return;
        }
        let from_path = match self.child_path(parent, name) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        let to_path = match self.child_path(newparent, newname) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        match self.core.rename(&from_path, &to_path) {
            Ok(()) => match self.write_inodes() {
                Ok(mut table) => {
                    table.remove_subtree(&to_path);
                    if let Ok(name) = validate_child_name(newname) {
                        let _ =
                            table.rename_subtree(&from_path, &to_path, newparent, name.to_string());
                    }
                    reply.ok();
                }
                Err(error) => reply.error(errno_for_error(&error)),
            },
            Err(error) => {
                trace_fuse_event("rename_error", error.to_string());
                reply.error(errno_for_error(&error));
            }
        }
    }
    fn link(
        &self,
        _req: &fuser::Request,
        _ino: fuser::INodeNo,
        _newparent: fuser::INodeNo,
        _newname: &OsStr,
        reply: fuser::ReplyEntry,
    ) {
        reply.error(fuser::Errno::ENOSYS);
    }

    fn read(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        offset: u64,
        size: u32,
        _flags: fuser::OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: fuser::ReplyData,
    ) {
        trace_fuse_event("read", format!("ino={ino:?} offset={offset} size={size}"));
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if entry.is_dir {
            reply.error(fuser::Errno::EISDIR);
            return;
        }
        match self.core.read_file(&entry.path, offset, size) {
            Ok(data) => reply.data(&data),
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn write(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: fuser::WriteFlags,
        _flags: fuser::OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: fuser::ReplyWrite,
    ) {
        trace_fuse_event(
            "write",
            format!("ino={ino:?} offset={offset} size={}", data.len()),
        );
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if entry.is_dir {
            reply.error(fuser::Errno::EISDIR);
            return;
        }
        match self.core.write_file(&entry.path, offset, data) {
            Ok(bytes_written) => {
                if let Ok(stat) = self.core.stat(&entry.path) {
                    if let Err(error) = self.refresh_inode_stat(entry, stat) {
                        reply.error(errno_for_error(&error));
                        return;
                    }
                }
                reply.written(bytes_written.min(u32::MAX as u64) as u32);
            }
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }

    fn readdir(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        offset: u64,
        mut reply: fuser::ReplyDirectory,
    ) {
        trace_fuse_event("readdir", format!("ino={ino:?} offset={offset}"));
        let Some(parent) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if !parent.is_dir {
            reply.error(fuser::Errno::ENOTDIR);
            return;
        }

        let list = match self.core.list_dir(&parent.path) {
            Ok(list) => list,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };

        let mut entries = vec![
            (parent.ino, fuser::FileType::Directory, ".".to_string()),
            (parent.parent, fuser::FileType::Directory, "..".to_string()),
        ];

        {
            let mut table = match self.write_inodes() {
                Ok(table) => table,
                Err(error) => {
                    reply.error(errno_for_error(&error));
                    return;
                }
            };
            for item in list {
                let kind = if item.stat.is_dir {
                    fuser::FileType::Directory
                } else {
                    fuser::FileType::RegularFile
                };
                let stat = item.stat;
                match table.upsert(parent.ino, item.name.clone(), stat) {
                    Ok(entry) => entries.push((entry.ino, kind, item.name)),
                    Err(error) => {
                        reply.error(errno_for_error(&error));
                        return;
                    }
                }
            }
        }

        let skip = usize::try_from(offset).unwrap_or(usize::MAX);
        for (index, (ino, kind, name)) in entries.into_iter().enumerate().skip(skip) {
            if reply.add(ino, (index + 1) as u64, kind, name) {
                break;
            }
        }
        reply.ok();
    }

    fn opendir(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        flags: fuser::OpenFlags,
        reply: fuser::ReplyOpen,
    ) {
        trace_fuse_event("opendir", format!("ino={ino:?} flags={flags:?}"));
        match self.inode(ino) {
            Some(entry) if entry.is_dir => {
                reply.opened(
                    fuser::FileHandle(u64::from(ino)),
                    fuser::FopenFlags::empty(),
                );
            }
            Some(_) => reply.error(fuser::Errno::ENOTDIR),
            None => reply.error(fuser::Errno::ENOENT),
        }
    }

    fn releasedir(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        _flags: fuser::OpenFlags,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event("releasedir", format!("ino={ino:?}"));
        reply.ok();
    }

    fn release(
        &self,
        _req: &fuser::Request,
        _ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        _flags: fuser::OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn flush(
        &self,
        _req: &fuser::Request,
        _ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        _lock_owner: fuser::LockOwner,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn fsync(
        &self,
        _req: &fuser::Request,
        _ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn fsyncdir(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        _fh: fuser::FileHandle,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event("fsyncdir", format!("ino={ino:?}"));
        reply.ok();
    }

    fn statfs(&self, _req: &fuser::Request, ino: fuser::INodeNo, reply: fuser::ReplyStatfs) {
        trace_fuse_event(
            "statfs",
            format!(
                "ino={ino:?} blocks=1048576 bfree=1048576 bavail=1048576 files=1000000 ffree=0 bsize=1 namelen=255 frsize=1"
            ),
        );
        reply.statfs(1_048_576, 1_048_576, 1_048_576, 1_000_000, 0, 1, 255, 1);
    }

    fn access(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        mask: fuser::AccessFlags,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event("access", format!("ino={ino:?} mask={mask}"));
        match self.access_errno(ino) {
            Some(errno) => reply.error(errno),
            None => reply.ok(),
        }
    }

    fn getxattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        name: &OsStr,
        _size: u32,
        reply: fuser::ReplyXattr,
    ) {
        trace_fuse_event("getxattr", format!("ino={ino:?} name={name:?}"));
        match xattr_decision(self.access_errno(ino), XattrRequest::Get) {
            XattrDecision::Error(errno) => reply.error(errno),
            XattrDecision::EmptyListSize | XattrDecision::EmptyListData => {
                reply.error(fuser::Errno::ENOTSUP)
            }
        }
    }

    fn listxattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        trace_fuse_event("listxattr", format!("ino={ino:?} size={size}"));
        match xattr_decision(self.access_errno(ino), XattrRequest::List { size }) {
            XattrDecision::Error(errno) => reply.error(errno),
            XattrDecision::EmptyListSize => reply.size(0),
            XattrDecision::EmptyListData => reply.data(&[]),
        }
    }

    fn setxattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        name: &OsStr,
        _value: &[u8],
        _flags: i32,
        _position: u32,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event("setxattr", format!("ino={ino:?} name={name:?}"));
        match xattr_decision(self.access_errno(ino), XattrRequest::Set) {
            XattrDecision::Error(errno) => reply.error(errno),
            XattrDecision::EmptyListSize | XattrDecision::EmptyListData => {
                reply.error(fuser::Errno::ENOTSUP)
            }
        }
    }

    fn removexattr(
        &self,
        _req: &fuser::Request,
        ino: fuser::INodeNo,
        name: &OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        trace_fuse_event("removexattr", format!("ino={ino:?} name={name:?}"));
        match xattr_decision(self.access_errno(ino), XattrRequest::Remove) {
            XattrDecision::Error(errno) => reply.error(errno),
            XattrDecision::EmptyListSize | XattrDecision::EmptyListData => {
                reply.error(fuser::Errno::ENOTSUP)
            }
        }
    }

    fn create(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let path = match self.child_path(parent, name) {
            Ok(path) => path,
            Err(error) => {
                reply.error(errno_for_error(&error));
                return;
            }
        };
        match self
            .core
            .create_file(&path)
            .and_then(|stat| self.upsert_child(parent, name, stat))
        {
            Ok(entry) => reply.created(
                &TTL,
                &file_attr(&entry),
                fuser::Generation(0),
                fuser::FileHandle(u64::from(entry.ino)),
                fuser::FopenFlags::empty(),
            ),
            Err(error) => reply.error(errno_for_error(&error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        ffi::OsStr,
        sync::{Arc, Mutex},
    };

    use operon_core::FsList;

    use super::*;

    #[test]
    fn mount_capability_constant_is_exported_at_crate_root() {
        assert_eq!(crate::MOUNT_CAPABILITY, "mount");
    }

    #[test]
    fn lookup_child_fetches_remote_stat_and_caches_inode() {
        let remote = Arc::new(MockRemoteFs::new([file_stat("/file.txt", 12)]));
        let fs = OperonFuseFs::new(remote.clone(), dir_stat("/"));

        let entry = fs
            .lookup_child(fuser::INodeNo::ROOT, OsStr::new("file.txt"))
            .expect("lookup child");
        let attr = file_attr(&entry);

        assert_eq!(entry.path, "/file.txt");
        assert_eq!(entry.size, 12);
        assert_eq!(attr.kind, fuser::FileType::RegularFile);
        assert_eq!(attr.size, 12);
        assert_eq!(remote.stat_calls(), vec!["/file.txt".to_string()]);
        assert_eq!(fs.inode(entry.ino).expect("cached inode").path, "/file.txt");
    }

    #[test]
    fn lookup_child_rejects_escape_names_before_remote_stat() {
        let remote = Arc::new(MockRemoteFs::new([]));
        let fs = OperonFuseFs::new(remote.clone(), dir_stat("/"));

        let error = fs
            .lookup_child(fuser::INodeNo::ROOT, OsStr::new(".."))
            .expect_err("escape child name should be rejected");

        assert!(error.to_string().contains("invalid child path name"));
        assert!(remote.stat_calls().is_empty());
    }

    #[test]
    fn access_accepts_known_inodes_and_rejects_missing_inodes() {
        let remote = Arc::new(MockRemoteFs::new([]));
        let fs = OperonFuseFs::new(remote, dir_stat("/"));

        assert!(fs.access_errno(fuser::INodeNo::ROOT).is_none());
        assert_eq!(
            fs.access_errno(fuser::INodeNo(99_999))
                .map(|errno| format!("{errno:?}")),
            Some(format!("{:?}", fuser::Errno::ENOENT))
        );
    }

    #[test]
    fn refresh_inode_stat_updates_cached_write_and_truncate_metadata() {
        let remote = Arc::new(MockRemoteFs::new([file_stat("/file.txt", 12)]));
        let fs = OperonFuseFs::new(remote, dir_stat("/"));
        let entry = fs
            .lookup_child(fuser::INodeNo::ROOT, OsStr::new("file.txt"))
            .expect("lookup child");

        let refreshed = fs
            .refresh_inode_stat(entry.clone(), file_stat("/file.txt", 64))
            .expect("refresh cached inode");

        assert_eq!(refreshed.ino, entry.ino);
        assert_eq!(refreshed.size, 64);
        assert_eq!(fs.inode(entry.ino).expect("cached inode").size, 64);
    }

    struct MockRemoteFs {
        stats: BTreeMap<String, FsStat>,
        stat_calls: Mutex<Vec<String>>,
    }

    impl MockRemoteFs {
        fn new(stats: impl IntoIterator<Item = FsStat>) -> Self {
            Self {
                stats: stats
                    .into_iter()
                    .map(|stat| (stat.path.clone(), stat))
                    .collect(),
                stat_calls: Mutex::new(Vec::new()),
            }
        }

        fn stat_calls(&self) -> Vec<String> {
            self.stat_calls.lock().expect("stat calls").clone()
        }
    }

    impl RemoteFs for MockRemoteFs {
        fn stat(&self, path: &str) -> anyhow::Result<FsStat> {
            self.stat_calls
                .lock()
                .expect("stat calls")
                .push(path.to_string());
            self.stats
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing stat for {path}"))
        }

        fn list(&self, _path: &str) -> anyhow::Result<FsList> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn read_range(&self, _path: &str, _offset: u64, _size: u32) -> anyhow::Result<Vec<u8>> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn write_range(&self, _path: &str, _offset: u64, _data: &[u8]) -> anyhow::Result<u64> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn truncate(&self, _path: &str, _size: u64) -> anyhow::Result<FsStat> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn mkdir(&self, _path: &str) -> anyhow::Result<FsStat> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn delete(&self, _path: &str) -> anyhow::Result<()> {
            unimplemented!("not needed by focused FUSE helper tests")
        }

        fn rename(&self, _from_path: &str, _to_path: &str) -> anyhow::Result<()> {
            unimplemented!("not needed by focused FUSE helper tests")
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
