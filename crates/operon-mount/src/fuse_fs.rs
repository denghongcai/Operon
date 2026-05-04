#![cfg(target_os = "linux")]

use std::{
    ffi::OsStr,
    path::Path,
    sync::{Arc, RwLock, RwLockWriteGuard},
    time::{Duration, UNIX_EPOCH},
};

use operon_core::FsStat;

use crate::{
    errors::errno_for_error,
    inode_table::{InodeEntry, InodeTable},
    path::{join_remote_child, validate_child_name},
    remote_client::RemoteFs,
};

const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 4096;

pub(crate) struct OperonFuseFs {
    remote_fs: Arc<dyn RemoteFs>,
    inodes: RwLock<InodeTable>,
}

impl OperonFuseFs {
    pub(crate) fn new(remote_fs: Arc<dyn RemoteFs>, root: FsStat) -> Self {
        Self {
            remote_fs,
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
        let stat = self.remote_fs.stat(&path)?;
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

    fn file_attr(&self, entry: &InodeEntry) -> fuser::FileAttr {
        fuser::FileAttr {
            ino: entry.ino,
            size: if entry.is_dir { 0 } else { entry.size },
            blocks: entry.size.div_ceil(BLOCK_SIZE as u64),
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: if entry.is_dir {
                fuser::FileType::Directory
            } else {
                fuser::FileType::RegularFile
            },
            perm: if entry.is_dir { 0o755 } else { 0o644 },
            nlink: if entry.is_dir { 2 } else { 1 },
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
            blksize: BLOCK_SIZE,
        }
    }
}

impl fuser::Filesystem for OperonFuseFs {
    fn lookup(
        &self,
        _req: &fuser::Request,
        parent: fuser::INodeNo,
        name: &OsStr,
        reply: fuser::ReplyEntry,
    ) {
        match self.lookup_child(parent, name) {
            Ok(entry) => reply.entry(&TTL, &self.file_attr(&entry), fuser::Generation(0)),
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
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };

        match self.remote_fs.stat(&entry.path) {
            Ok(stat) => {
                let refreshed = self
                    .write_inodes()
                    .and_then(|mut table| table.upsert(entry.parent, entry.name, stat));
                match refreshed {
                    Ok(entry) => reply.attr(&TTL, &self.file_attr(&entry)),
                    Err(error) => reply.error(errno_for_error(&error)),
                }
            }
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
            match self.remote_fs.truncate(&entry.path, size).and_then(|stat| {
                self.write_inodes()
                    .and_then(|mut table| table.upsert(entry.parent, entry.name.clone(), stat))
            }) {
                Ok(entry) => reply.attr(&TTL, &self.file_attr(&entry)),
                Err(error) => reply.error(errno_for_error(&error)),
            }
            return;
        }

        reply.attr(&TTL, &self.file_attr(&entry));
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
            .remote_fs
            .mkdir(&path)
            .and_then(|stat| self.upsert_child(parent, name, stat))
        {
            Ok(entry) => reply.entry(&TTL, &self.file_attr(&entry), fuser::Generation(0)),
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
        match self.remote_fs.delete(&path) {
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
        match self.remote_fs.delete(&path) {
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
        if !flags.is_empty() {
            reply.error(fuser::Errno::ENOSYS);
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
        match self.remote_fs.rename(&from_path, &to_path) {
            Ok(()) => match self.write_inodes() {
                Ok(mut table) => {
                    table.remove_subtree(&from_path);
                    table.remove_subtree(&to_path);
                    reply.ok();
                }
                Err(error) => reply.error(errno_for_error(&error)),
            },
            Err(error) => reply.error(errno_for_error(&error)),
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
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if entry.is_dir {
            reply.error(fuser::Errno::EISDIR);
            return;
        }
        match self.remote_fs.read_range(&entry.path, offset, size) {
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
        let Some(entry) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if entry.is_dir {
            reply.error(fuser::Errno::EISDIR);
            return;
        }
        match self.remote_fs.write_range(&entry.path, offset, data) {
            Ok(bytes_written) => {
                if let Ok(stat) = self.remote_fs.stat(&entry.path) {
                    match self.write_inodes() {
                        Ok(mut table) => {
                            let _ = table.upsert(entry.parent, entry.name, stat);
                        }
                        Err(error) => {
                            reply.error(errno_for_error(&error));
                            return;
                        }
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
        let Some(parent) = self.inode(ino) else {
            reply.error(fuser::Errno::ENOENT);
            return;
        };
        if !parent.is_dir {
            reply.error(fuser::Errno::ENOTDIR);
            return;
        }

        let list = match self.remote_fs.list(&parent.path) {
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
            for item in list.entries {
                let kind = if item.is_dir {
                    fuser::FileType::Directory
                } else {
                    fuser::FileType::RegularFile
                };
                let stat = FsStat {
                    path: item.path,
                    is_file: item.is_file,
                    is_dir: item.is_dir,
                    size: item.size,
                    version: item.version,
                };
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
            .remote_fs
            .write_range(&path, 0, &[])
            .and_then(|_| self.remote_fs.stat(&path))
            .and_then(|stat| self.upsert_child(parent, name, stat))
        {
            Ok(entry) => reply.created(
                &TTL,
                &self.file_attr(&entry),
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
        let attr = fs.file_attr(&entry);

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
    fn file_attr_maps_directories_with_directory_permissions() {
        let remote = Arc::new(MockRemoteFs::new([]));
        let fs = OperonFuseFs::new(remote, dir_stat("/"));
        let entry = fs.inode(fuser::INodeNo::ROOT).expect("root inode");

        let attr = fs.file_attr(&entry);

        assert_eq!(attr.kind, fuser::FileType::Directory);
        assert_eq!(attr.perm, 0o755);
        assert_eq!(attr.nlink, 2);
        assert_eq!(attr.size, 0);
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
