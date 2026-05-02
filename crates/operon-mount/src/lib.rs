#![cfg(target_os = "linux")]

use std::{
    collections::HashMap,
    ffi::OsStr,
    future::Future,
    panic,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc, RwLock,
    },
    time::{Duration, UNIX_EPOCH},
};

use anyhow::Context;
use operon_core::{FsList, FsStat};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    operon_runtime_client::OperonRuntimeClient, FsPathRequest, FsReadRangeRequest, FsRenameRequest,
    FsTruncateRequest, FsWriteRangeRequest,
};
use tonic::{metadata::MetadataValue, transport::Channel, Code, Request, Status};

pub const MOUNT_CAPABILITY: &str = "mount";

const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 4096;

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
        ctrlc::set_handler(move || {
            let _ = tx.send(());
        })
        .context("failed to install mount shutdown handler")?;

        loop {
            match rx.recv_timeout(Duration::from_secs(3600)) {
                Ok(()) => return self.unmount(),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return self.unmount(),
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

struct GrpcRemoteFs {
    endpoint: NodeEndpoint,
    channel: Channel,
    runtime: Option<tokio::runtime::Runtime>,
}

impl GrpcRemoteFs {
    fn connect(endpoint: NodeEndpoint) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let uri = grpc_channel_uri(&endpoint.endpoint)?;
        let builder = Channel::from_shared(uri)?;
        let channel = block_on_runtime(&runtime, async { builder.connect().await })?;
        Ok(Self {
            endpoint,
            channel,
            runtime: Some(runtime),
        })
    }

    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .as_ref()
            .expect("remote fs runtime is only cleared during drop")
    }
}

impl RemoteFs for GrpcRemoteFs {
    fn stat(&self, path: &str) -> anyhow::Result<FsStat> {
        let path = path.to_string();
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .stat_fs(with_auth(&self.endpoint, FsPathRequest { path })?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<FsList> {
        let path = path.to_string();
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .list_fs(with_auth(&self.endpoint, FsPathRequest { path })?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>> {
        let request = FsReadRangeRequest {
            path: path.to_string(),
            offset,
            size,
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .read_file_range(with_auth(&self.endpoint, request)?)
                .await?
                .into_inner()
                .data)
        })
    }

    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        let request = FsWriteRangeRequest {
            path: path.to_string(),
            offset,
            data: data.to_vec(),
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .write_file_range(with_auth(&self.endpoint, request)?)
                .await?
                .into_inner()
                .bytes_written)
        })
    }

    fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat> {
        let request = FsTruncateRequest {
            path: path.to_string(),
            size,
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .truncate_fs(with_auth(&self.endpoint, request)?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn mkdir(&self, path: &str) -> anyhow::Result<FsStat> {
        let request = FsPathRequest {
            path: path.to_string(),
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .mkdir_fs(with_auth(&self.endpoint, request)?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn delete(&self, path: &str) -> anyhow::Result<()> {
        let request = FsPathRequest {
            path: path.to_string(),
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            client
                .delete_fs(with_auth(&self.endpoint, request)?)
                .await?;
            Ok(())
        })
    }

    fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()> {
        let request = FsRenameRequest {
            from_path: from_path.to_string(),
            to_path: to_path.to_string(),
        };
        block_on_runtime(self.runtime(), async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            client
                .rename_fs(with_auth(&self.endpoint, request)?)
                .await?;
            Ok(())
        })
    }
}

impl Drop for GrpcRemoteFs {
    fn drop(&mut self) {
        let Some(runtime) = self.runtime.take() else {
            return;
        };

        if tokio::runtime::Handle::try_current().is_err() {
            drop(runtime);
            return;
        }

        match std::thread::spawn(move || drop(runtime)).join() {
            Ok(()) => {}
            Err(payload) => panic::resume_unwind(payload),
        }
    }
}

fn with_auth<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<Request<T>> {
    let mut request = Request::new(message);
    if let Some(token) = &endpoint.token {
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))?,
        );
    }
    Ok(request)
}

fn grpc_channel_uri(endpoint: &str) -> anyhow::Result<String> {
    if let Some(rest) = endpoint.strip_prefix("grpc://") {
        Ok(format!("http://{rest}"))
    } else if let Some(rest) = endpoint.strip_prefix("grpcs://") {
        Ok(format!("https://{rest}"))
    } else {
        anyhow::bail!("only grpc:// and grpcs:// endpoints are supported by the mount client")
    }
}

#[derive(Debug, Clone)]
struct InodeEntry {
    ino: fuser::INodeNo,
    parent: fuser::INodeNo,
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
}

#[derive(Debug)]
struct InodeTable {
    next: u64,
    by_ino: HashMap<u64, InodeEntry>,
    by_path: HashMap<String, u64>,
}

impl InodeTable {
    fn new(root: FsStat) -> Self {
        let root_entry = InodeEntry {
            ino: fuser::INodeNo::ROOT,
            parent: fuser::INodeNo::ROOT,
            name: ".".to_string(),
            path: root.path.clone(),
            is_dir: true,
            size: root.size,
        };
        let mut by_ino = HashMap::new();
        let mut by_path = HashMap::new();
        by_path.insert(root.path, u64::from(fuser::INodeNo::ROOT));
        by_ino.insert(u64::from(fuser::INodeNo::ROOT), root_entry);
        Self {
            next: u64::from(fuser::INodeNo::ROOT) + 1,
            by_ino,
            by_path,
        }
    }

    fn get(&self, ino: fuser::INodeNo) -> Option<InodeEntry> {
        self.by_ino.get(&u64::from(ino)).cloned()
    }

    fn remove_subtree(&mut self, path: &str) {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{path}/")
        };
        let removed: Vec<u64> = self
            .by_path
            .iter()
            .filter_map(|(entry_path, ino)| {
                (entry_path == path || entry_path.starts_with(&prefix)).then_some(*ino)
            })
            .collect();
        for ino in removed {
            if let Some(entry) = self.by_ino.remove(&ino) {
                self.by_path.remove(&entry.path);
            }
        }
    }

    fn upsert(
        &mut self,
        parent: fuser::INodeNo,
        name: String,
        stat: FsStat,
    ) -> anyhow::Result<InodeEntry> {
        let path = normalize_remote_path(&stat.path)?;
        let ino = if let Some(ino) = self.by_path.get(&path) {
            fuser::INodeNo(*ino)
        } else {
            let ino = fuser::INodeNo(self.next);
            self.next += 1;
            self.by_path.insert(path.clone(), u64::from(ino));
            ino
        };
        let entry = InodeEntry {
            ino,
            parent,
            name,
            path: path.clone(),
            is_dir: stat.is_dir,
            size: stat.size,
        };
        self.by_ino.insert(u64::from(ino), entry.clone());
        Ok(entry)
    }
}

struct OperonFuseFs {
    remote_fs: Arc<dyn RemoteFs>,
    inodes: RwLock<InodeTable>,
}

impl OperonFuseFs {
    fn new(remote_fs: Arc<dyn RemoteFs>, root: FsStat) -> Self {
        Self {
            remote_fs,
            inodes: RwLock::new(InodeTable::new(root)),
        }
    }

    fn inode(&self, ino: fuser::INodeNo) -> Option<InodeEntry> {
        self.inodes.read().ok()?.get(ino)
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
        self.inodes
            .write()
            .expect("inode table poisoned")
            .upsert(parent, name.to_string(), stat)
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
        self.inodes
            .write()
            .expect("inode table poisoned")
            .upsert(parent, name.to_string(), stat)
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
                let refreshed = self.inodes.write().expect("inode table poisoned").upsert(
                    entry.parent,
                    entry.name,
                    stat,
                );
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
                self.inodes.write().expect("inode table poisoned").upsert(
                    entry.parent,
                    entry.name.clone(),
                    stat,
                )
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
            Ok(()) => {
                self.inodes
                    .write()
                    .expect("inode table poisoned")
                    .remove_subtree(&path);
                reply.ok();
            }
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
            Ok(()) => {
                self.inodes
                    .write()
                    .expect("inode table poisoned")
                    .remove_subtree(&path);
                reply.ok();
            }
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
            Ok(()) => {
                let mut table = self.inodes.write().expect("inode table poisoned");
                table.remove_subtree(&from_path);
                table.remove_subtree(&to_path);
                reply.ok();
            }
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
                    let _ = self.inodes.write().expect("inode table poisoned").upsert(
                        entry.parent,
                        entry.name,
                        stat,
                    );
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
            let mut table = self.inodes.write().expect("inode table poisoned");
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

fn errno_for_error(error: &anyhow::Error) -> fuser::Errno {
    if let Some(status) = error.downcast_ref::<Status>() {
        return match status.code() {
            Code::NotFound => fuser::Errno::ENOENT,
            Code::PermissionDenied | Code::Unauthenticated => fuser::Errno::EACCES,
            Code::AlreadyExists => fuser::Errno::EEXIST,
            Code::InvalidArgument | Code::FailedPrecondition => fuser::Errno::EINVAL,
            Code::Unimplemented => fuser::Errno::ENOSYS,
            _ => fuser::Errno::EIO,
        };
    }

    let message = error.to_string();
    if message.contains("not found") {
        fuser::Errno::ENOENT
    } else if message.contains("permission") || message.contains("denied") {
        fuser::Errno::EACCES
    } else {
        fuser::Errno::EIO
    }
}

fn normalize_remote_path(path: &str) -> anyhow::Result<String> {
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

fn block_on_runtime<F, T>(runtime: &tokio::runtime::Runtime, future: F) -> T
where
    F: Future<Output = T> + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_err() {
        return runtime.block_on(future);
    }

    std::thread::scope(
        |scope| match scope.spawn(|| runtime.block_on(future)).join() {
            Ok(result) => result,
            Err(payload) => panic::resume_unwind(payload),
        },
    )
}

fn validate_child_name(name: &OsStr) -> anyhow::Result<&str> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path names must be valid UTF-8"))?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.as_bytes().contains(&0)
    {
        anyhow::bail!("invalid child path name");
    }
    Ok(name)
}

fn join_remote_child(parent: &str, child: &str) -> anyhow::Result<String> {
    let parent = normalize_remote_path(parent)?;
    if child.is_empty() || child == "." || child == ".." || child.contains('/') {
        anyhow::bail!("invalid child path name");
    }
    if parent == "/" {
        Ok(format!("/{child}"))
    } else {
        Ok(format!("{parent}/{child}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_capability_id_is_stable() {
        assert_eq!(MOUNT_CAPABILITY, "mount");
    }

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
        assert!(error.to_string().contains("`..`"));
    }

    #[test]
    fn joins_remote_children_under_root() {
        assert_eq!(
            join_remote_child("/", "file.txt").expect("join"),
            "/file.txt"
        );
        assert_eq!(
            join_remote_child("/workspace", "file.txt").expect("join"),
            "/workspace/file.txt"
        );
    }

    #[test]
    fn rejects_invalid_child_names() {
        assert!(join_remote_child("/workspace", "../secret").is_err());
        assert!(join_remote_child("/workspace", "dir/file").is_err());
    }

    #[test]
    fn inode_table_reuses_paths() {
        let root = FsStat {
            path: "/".to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
        };
        let mut table = InodeTable::new(root);
        let first = table
            .upsert(
                fuser::INodeNo::ROOT,
                "a.txt".to_string(),
                FsStat {
                    path: "/a.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 10,
                },
            )
            .expect("first");
        let second = table
            .upsert(
                fuser::INodeNo::ROOT,
                "a.txt".to_string(),
                FsStat {
                    path: "/a.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 12,
                },
            )
            .expect("second");

        assert_eq!(first.ino, second.ino);
        assert_eq!(second.size, 12);
    }

    #[test]
    fn inode_table_removes_subtrees() {
        let root = FsStat {
            path: "/".to_string(),
            is_file: false,
            is_dir: true,
            size: 0,
        };
        let mut table = InodeTable::new(root);
        let dir = table
            .upsert(
                fuser::INodeNo::ROOT,
                "dir".to_string(),
                FsStat {
                    path: "/dir".to_string(),
                    is_file: false,
                    is_dir: true,
                    size: 0,
                },
            )
            .expect("dir");
        let file = table
            .upsert(
                dir.ino,
                "file.txt".to_string(),
                FsStat {
                    path: "/dir/file.txt".to_string(),
                    is_file: true,
                    is_dir: false,
                    size: 4,
                },
            )
            .expect("file");

        table.remove_subtree("/dir");

        assert!(table.get(dir.ino).is_none());
        assert!(table.get(file.ino).is_none());
        assert!(table.get(fuser::INodeNo::ROOT).is_some());
    }

    #[test]
    fn converts_grpc_mount_uri_to_tonic_uri() {
        assert_eq!(
            grpc_channel_uri("grpc://127.0.0.1:7789").expect("uri"),
            "http://127.0.0.1:7789"
        );
    }

    #[test]
    fn block_on_runtime_can_run_inside_an_existing_tokio_runtime() {
        let outer = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("outer runtime");
        let inner = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("inner runtime");

        let value = outer.block_on(async { block_on_runtime(&inner, async { 42 }) });

        assert_eq!(value, 42);
    }

    #[test]
    fn nested_runtime_can_drop_inside_an_existing_tokio_runtime() {
        let outer = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("outer runtime");

        outer.block_on(async {
            let remote_fs = GrpcRemoteFs {
                endpoint: NodeEndpoint {
                    node_id: "node".to_string(),
                    endpoint: "grpc://127.0.0.1:7788".to_string(),
                    provider: operon_network::NetworkProviderKind::Lan,
                    token: None,
                },
                channel: Channel::from_static("http://127.0.0.1:7788").connect_lazy(),
                runtime: Some(
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("inner runtime"),
                ),
            };
            drop(remote_fs);
        });
    }
}
