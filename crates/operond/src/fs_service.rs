use std::{path::PathBuf, pin::Pin};

use futures_util::{Stream, StreamExt};
use operon_core::{FsEntry, FsList, FsPrecondition, FsStat, FsWrite};
use operon_fs::{authorize_fs_decision, join_virtual_path};
use operon_protocol::runtime::v1::{write_file_request, FileChunk, WriteFileRequest};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use tonic::Status;

use crate::{
    audit::record_policy_decision,
    grpc_status::{status_from_error, status_from_io_error},
    record_audit, AppState, MAX_FS_FILE_BYTES, MAX_FS_WRITE_CHUNK_BYTES,
};

pub(crate) type FileStream =
    Pin<Box<dyn Stream<Item = Result<FileChunk, Status>> + Send + 'static>>;

// Serializes daemon-owned mutations, including aliases and directory renames.
// External filesystem writers are outside this coordination boundary.
static MUTATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) fn validate_write_chunk(data_len: usize) -> Result<(), Status> {
    if data_len > MAX_FS_WRITE_CHUNK_BYTES {
        return Err(Status::invalid_argument(format!(
            "fs write chunk exceeds {} bytes",
            MAX_FS_WRITE_CHUNK_BYTES
        )));
    }
    Ok(())
}

pub(crate) fn validate_read_range_size(size: u32) -> Result<(), Status> {
    let data_len = usize::try_from(size)
        .map_err(|_| Status::invalid_argument("read range size is too large"))?;
    if data_len > MAX_FS_WRITE_CHUNK_BYTES {
        return Err(Status::invalid_argument(format!(
            "fs read range exceeds {} bytes",
            MAX_FS_WRITE_CHUNK_BYTES
        )));
    }
    Ok(())
}

pub(crate) fn checked_file_end(
    offset: u64,
    data_len: usize,
    operation: &str,
) -> Result<u64, Status> {
    let len = u64::try_from(data_len)
        .map_err(|_| Status::invalid_argument(format!("{operation} data length is too large")))?;
    let end = offset.checked_add(len).ok_or_else(|| {
        Status::invalid_argument(format!("{operation} offset plus data length overflows"))
    })?;
    if end > MAX_FS_FILE_BYTES {
        return Err(Status::invalid_argument(format!(
            "{operation} exceeds maximum fs object size of {} bytes",
            MAX_FS_FILE_BYTES
        )));
    }
    Ok(end)
}

fn fs_version(metadata: &std::fs::Metadata) -> String {
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let kind = if metadata.is_dir() {
        "dir"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    format!("v1:{kind}:{}:{modified_nanos}", metadata.len())
}

fn version_from_metadata(metadata: std::fs::Metadata) -> String {
    fs_version(&metadata)
}

fn grpc_precondition(
    precondition: Option<operon_protocol::runtime::v1::FsPrecondition>,
    expected_version: Option<String>,
    require_absent: bool,
) -> Option<FsPrecondition> {
    let mut precondition = precondition.map(Into::into);
    if expected_version.is_some() || require_absent {
        let current = precondition.get_or_insert(FsPrecondition {
            expected_version: None,
            require_absent: false,
        });
        if expected_version.is_some() {
            current.expected_version = expected_version;
        }
        current.require_absent |= require_absent;
    }
    precondition
}

pub(crate) fn precondition_from_path_request(
    request: operon_protocol::runtime::v1::FsPathRequest,
) -> (String, Option<FsPrecondition>) {
    (request.path, request.precondition.map(Into::into))
}

pub(crate) fn precondition_from_write_range_request(
    request: operon_protocol::runtime::v1::FsWriteRangeRequest,
) -> (String, u64, Vec<u8>, Option<FsPrecondition>) {
    (
        request.path,
        request.offset,
        request.data,
        grpc_precondition(
            request.precondition,
            request.expected_version,
            request.require_absent,
        ),
    )
}

pub(crate) fn precondition_from_truncate_request(
    request: operon_protocol::runtime::v1::FsTruncateRequest,
) -> (String, u64, Option<FsPrecondition>) {
    (
        request.path,
        request.size,
        grpc_precondition(
            request.precondition,
            request.expected_version,
            request.require_absent,
        ),
    )
}

pub(crate) fn preconditions_from_rename_request(
    request: &operon_protocol::runtime::v1::FsRenameRequest,
) -> (Option<FsPrecondition>, Option<FsPrecondition>) {
    (
        grpc_precondition(
            request.from_precondition.clone(),
            request.from_expected_version.clone(),
            false,
        ),
        grpc_precondition(
            request.to_precondition.clone(),
            request.to_expected_version.clone(),
            request.to_require_absent,
        ),
    )
}

pub(crate) fn preconditions_from_copy_request(
    request: &operon_protocol::runtime::v1::FsCopyRequest,
) -> (Option<FsPrecondition>, Option<FsPrecondition>) {
    (
        grpc_precondition(
            request.from_precondition.clone(),
            request.from_expected_version.clone(),
            false,
        ),
        grpc_precondition(
            request.to_precondition.clone(),
            request.to_expected_version.clone(),
            request.to_require_absent,
        ),
    )
}

fn check_precondition(path: &PathBuf, precondition: Option<&FsPrecondition>) -> Result<(), Status> {
    let Some(precondition) = precondition else {
        return Ok(());
    };
    if precondition.require_absent {
        match std::fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(Status::failed_precondition(
                    "fs precondition failed: target already exists",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(status_from_io_error(error)),
        }
    }
    let Some(expected_version) = precondition.expected_version.as_deref() else {
        return Ok(());
    };
    let metadata = std::fs::symlink_metadata(path).map_err(status_from_io_error)?;
    let actual_version = fs_version(&metadata);
    if actual_version == expected_version {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "fs precondition failed: expected version {expected_version}, actual version {actual_version}"
    )))
}

fn authorize_fs_action(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    permission: &str,
    path: &str,
) -> Result<(), Status> {
    let mut decision = authorize_fs_decision(&state.policy, permission, path);
    decision.action = audit_action.to_string();
    decision.resource = audit_resource.to_string();
    if !decision.allowed {
        record_policy_decision(state, &decision);
        return Err(status_from_error(decision.runtime_error()));
    }
    Ok(())
}

fn resolve_existing_path(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    path: &str,
) -> Result<PathBuf, Status> {
    workspace_resolver(state)?.existing(path).map_err(|error| {
        record_audit(state, audit_action, audit_resource, false, &error.1);
        status_from_error(error)
    })
}

fn resolve_existing_leaf_path(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    path: &str,
) -> Result<PathBuf, Status> {
    workspace_resolver(state)?
        .existing_leaf(path)
        .map_err(|error| {
            record_audit(state, audit_action, audit_resource, false, &error.1);
            status_from_error(error)
        })
}

fn resolve_write_path(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    path: &str,
) -> Result<PathBuf, Status> {
    resolve_write_target(state, audit_action, audit_resource, path).map(|(path, _)| path)
}

fn resolve_write_target(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    path: &str,
) -> Result<(PathBuf, bool), Status> {
    workspace_resolver(state)?
        .write_with_parent(path)
        .map_err(|error| {
            record_audit(state, audit_action, audit_resource, false, &error.1);
            status_from_error(error)
        })
}

fn resolve_create_path(
    state: &AppState,
    audit_action: &str,
    audit_resource: &str,
    path: &str,
) -> Result<PathBuf, Status> {
    workspace_resolver(state)?.create(path).map_err(|error| {
        record_audit(state, audit_action, audit_resource, false, &error.1);
        status_from_error(error)
    })
}

fn workspace_resolver(state: &AppState) -> Result<&operon_fs::WorkspaceResolver, Status> {
    state
        .workspace_resolver
        .as_deref()
        .ok_or_else(|| Status::failed_precondition("workspace resolver is unavailable"))
}

pub(crate) async fn stat(state: &AppState, path: String) -> Result<FsStat, Status> {
    authorize_fs_action(state, "stat", &path, "read", &path)?;
    let full_path = resolve_existing_path(state, "stat", &path, &path)?;
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "stat", &path, true, "allowed");
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        version: version_from_metadata(metadata),
    })
}

pub(crate) async fn list_page(
    state: &AppState,
    path: String,
    page_size: u32,
    page_token: &str,
) -> Result<FsList, Status> {
    authorize_fs_action(state, "list", &path, "read", &path)?;
    let full_path = resolve_existing_path(state, "list", &path, &path)?;
    let mut entries = Vec::new();
    let mut reader = tokio::fs::read_dir(&full_path)
        .await
        .map_err(status_from_io_error)?;
    while let Some(entry) = reader.next_entry().await.map_err(status_from_io_error)? {
        let metadata = tokio::fs::symlink_metadata(entry.path())
            .await
            .map_err(status_from_io_error)?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child_path = join_virtual_path(&path, &name);
        entries.push(FsEntry {
            name,
            path: child_path,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            version: version_from_metadata(metadata),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let (entries, next_page_token) =
        crate::pagination::paginate_items(&entries, page_size, page_token)?;
    record_audit(state, "list", &path, true, "allowed");
    Ok(FsList {
        path,
        entries,
        next_page_token,
    })
}

pub(crate) async fn read_stream(state: &AppState, path: String) -> Result<FileStream, Status> {
    authorize_fs_action(state, "read-stream", &path, "read", &path)?;
    let full_path = resolve_existing_path(state, "read-stream", &path, &path)?;
    let file = tokio::fs::File::open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "read-stream", &path, true, "allowed");
    let stream = ReaderStream::with_capacity(file, 64 * 1024).map(|chunk| {
        chunk
            .map(|data| FileChunk {
                data: data.to_vec(),
            })
            .map_err(status_from_io_error)
    });
    Ok(Box::pin(stream))
}

pub(crate) async fn read_range(
    state: &AppState,
    path: String,
    offset: u64,
    size: u32,
) -> Result<FileChunk, Status> {
    validate_read_range_size(size)?;
    checked_file_end(offset, size as usize, "read range")?;
    authorize_fs_action(state, "read-range", &path, "read", &path)?;
    let full_path = resolve_existing_path(state, "read-range", &path, &path)?;
    let mut file = tokio::fs::File::open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(status_from_io_error)?;
    let mut data = vec![0_u8; size as usize];
    let bytes_read = fill_range(&mut file, &mut data)
        .await
        .map_err(status_from_io_error)?;
    data.truncate(bytes_read);
    record_audit(state, "read-range", &path, true, "allowed");
    Ok(FileChunk { data })
}

async fn fill_range<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    data: &mut [u8],
) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < data.len() {
        match reader.read(&mut data[filled..]).await {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

async fn preserve_replacement_permissions(
    path: &std::path::Path,
    file: &mut tokio::fs::File,
) -> Result<(), Status> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(error) => return Err(status_from_io_error(error)),
    };
    if !metadata.is_file() {
        return Err(Status::failed_precondition(
            "write target is not a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::{fs::MetadataExt, io::AsRawFd};
        let staged = file
            .try_clone()
            .await
            .map_err(status_from_io_error)?
            .into_std()
            .await;
        let source = path.to_path_buf();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let current = staged.metadata()?;
            if (current.uid() != metadata.uid() || current.gid() != metadata.gid())
                && unsafe { libc::fchown(staged.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            copy_access_acl(&source, &staged)?;
            staged.set_permissions(metadata.permissions())
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .map_err(status_from_io_error)?;
        Ok(())
    }
    #[cfg(not(unix))]
    file.set_permissions(metadata.permissions())
        .await
        .map_err(status_from_io_error)
}

#[cfg(target_os = "linux")]
fn copy_access_acl(source: &std::path::Path, destination: &std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let source = std::fs::OpenOptions::new().write(true).open(source)?;
    let name = c"system.posix_acl_access";
    let size =
        unsafe { libc::fgetxattr(source.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENODATA) {
            let result = unsafe { libc::fremovexattr(destination.as_raw_fd(), name.as_ptr()) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) {
                    return Err(error);
                }
            }
            return Ok(());
        }
        if error.raw_os_error() == Some(libc::ENOTSUP) {
            return Ok(());
        }
        return Err(error);
    }
    let mut acl = vec![0u8; size as usize];
    let size = unsafe {
        libc::fgetxattr(
            source.as_raw_fd(),
            name.as_ptr(),
            acl.as_mut_ptr().cast(),
            acl.len(),
        )
    };
    if size < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe {
        libc::fsetxattr(
            destination.as_raw_fd(),
            name.as_ptr(),
            acl.as_ptr().cast(),
            size as usize,
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_access_acl(source: &std::path::Path, destination: &std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn acl_get_fd(fd: libc::c_int) -> *mut libc::c_void;
        fn acl_set_fd(fd: libc::c_int, acl: *mut libc::c_void) -> libc::c_int;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    let source = std::fs::OpenOptions::new().write(true).open(source)?;
    let acl = unsafe { acl_get_fd(source.as_raw_fd()) };
    if acl.is_null() {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ENOTSUP)) {
            return Ok(());
        }
        return Err(error);
    }
    let result = unsafe { acl_set_fd(destination.as_raw_fd(), acl) };
    let error = std::io::Error::last_os_error();
    unsafe {
        acl_free(acl);
    }
    if result != 0 {
        return Err(error);
    }
    Ok(())
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn copy_access_acl(_source: &std::path::Path, _destination: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}

fn commit_replacement(
    staging: tempfile::TempPath,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
        if destination.exists() {
            let target = destination
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let source = staging
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            // ReplaceFile preserves the destination's DACL. Open-handle sharing
            // restrictions surface as ordinary commit errors, preserving target.
            let result = unsafe {
                ReplaceFileW(
                    target.as_ptr(),
                    source.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if result == 0 {
                return Err(std::io::Error::last_os_error());
            }
            return Ok(());
        }
    }
    if destination.exists() {
        staging.persist(destination).map_err(|e| e.error)
    } else {
        staging.persist_noclobber(destination).map_err(|e| e.error)
    }
}

struct StreamAuditGuard<'a> {
    state: &'a AppState,
    path: Option<String>,
    committed: bool,
}

impl Drop for StreamAuditGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(path) = &self.path {
                record_audit(
                    self.state,
                    "write-stream",
                    path,
                    false,
                    "stream ended without commit",
                );
            }
        }
    }
}

pub(crate) async fn write_stream<S>(state: &AppState, stream: &mut S) -> Result<FsWrite, Status>
where
    S: Stream<Item = Result<WriteFileRequest, Status>> + Unpin,
{
    write_stream_impl(
        state,
        stream,
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq)]
enum StreamIoFailure {
    Write,
    Flush,
    Commit,
}

#[cfg(test)]
fn inject_stream_io_failure(
    failure: Option<StreamIoFailure>,
    point: StreamIoFailure,
) -> Result<(), Status> {
    if failure == Some(point) {
        Err(status_from_io_error(std::io::Error::other(
            "injected stream I/O failure",
        )))
    } else {
        Ok(())
    }
}

async fn write_stream_impl<S>(
    state: &AppState,
    stream: &mut S,
    #[cfg(test)] failure: Option<StreamIoFailure>,
) -> Result<FsWrite, Status>
where
    S: Stream<Item = Result<WriteFileRequest, Status>> + Unpin,
{
    let mut path = None;
    let mut file = None;
    let mut bytes_written = 0_u64;
    let mut staging = None;
    let mut destination = None;
    let mut commit_precondition = None;
    let mut outcome = StreamAuditGuard {
        state,
        path: None,
        committed: false,
    };

    while let Some(message) = stream.next().await {
        let message = message?;
        match message.payload {
            Some(write_file_request::Payload::Target(target)) => {
                if path.is_some() {
                    return Err(Status::invalid_argument(
                        "write stream target metadata was sent more than once",
                    ));
                }
                if target.path.is_empty() {
                    return Err(Status::invalid_argument(
                        "write stream target path is required",
                    ));
                }
                authorize_fs_action(state, "write-stream", &target.path, "write", &target.path)?;
                outcome.path = Some(target.path.clone());
                let _mutation = MUTATION_LOCK.lock().await;
                let (full_path, parent_exists) =
                    resolve_write_target(state, "write-stream", &target.path, &target.path)?;
                let precondition = grpc_precondition(
                    target.precondition,
                    target.expected_version,
                    target.require_absent,
                );
                check_precondition(&full_path, precondition.as_ref())?;
                if let Some(parent) = full_path.parent().filter(|_| !parent_exists) {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(status_from_io_error)?;
                }
                let parent = full_path
                    .parent()
                    .ok_or_else(|| Status::invalid_argument("write target has no parent"))?
                    .to_path_buf();
                let staged = tokio::task::spawn_blocking(move || {
                    let mut builder = tempfile::Builder::new();
                    builder.prefix(".operon-write-");
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        builder.permissions(std::fs::Permissions::from_mode(0o666));
                    }
                    builder.tempfile_in(parent)
                })
                .await
                .map_err(|e| Status::internal(e.to_string()))?
                .map_err(status_from_io_error)?;
                let (std_file, temp_path) = staged.into_parts();
                let mut staged_file = tokio::fs::File::from_std(std_file);
                preserve_replacement_permissions(&full_path, &mut staged_file).await?;
                file = Some(staged_file);
                staging = Some(temp_path);
                destination = Some(full_path);
                commit_precondition = precondition;
                path = Some(target.path);
            }
            Some(write_file_request::Payload::Chunk(chunk)) => {
                let Some(file) = &mut file else {
                    return Err(Status::invalid_argument(
                        "write stream chunk arrived before target metadata",
                    ));
                };
                validate_write_chunk(chunk.data.len())?;
                bytes_written = checked_file_end(bytes_written, chunk.data.len(), "write stream")?;
                #[cfg(test)]
                inject_stream_io_failure(failure, StreamIoFailure::Write)?;
                file.write_all(&chunk.data)
                    .await
                    .map_err(status_from_io_error)?;
            }
            None => {
                return Err(Status::invalid_argument(
                    "write stream message is missing payload",
                ));
            }
        }
    }

    let Some(path) = path else {
        return Err(Status::invalid_argument(
            "write stream did not include target metadata",
        ));
    };
    let mut file = file.ok_or_else(|| Status::internal("missing staged file"))?;
    #[cfg(test)]
    inject_stream_io_failure(failure, StreamIoFailure::Flush)?;
    file.flush().await.map_err(status_from_io_error)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let full_path = resolve_write_path(state, "write-stream", &path, &path)?;
    if destination.as_ref() != Some(&full_path) {
        return Err(Status::failed_precondition(
            "write destination changed while streaming",
        ));
    }
    check_precondition(&full_path, commit_precondition.as_ref())?;
    preserve_replacement_permissions(&full_path, &mut file).await?;
    let metadata = file.metadata().await.map_err(status_from_io_error)?;
    drop(file);
    let staged = staging.ok_or_else(|| Status::internal("missing staged path"))?;
    // There is no cancellation point between the final decision and commit.
    #[cfg(test)]
    inject_stream_io_failure(failure, StreamIoFailure::Commit)?;
    commit_replacement(staged, &full_path).map_err(status_from_io_error)?;
    outcome.committed = true;
    drop(_mutation);
    record_audit(state, "write-stream", &path, true, "allowed");
    let stat = FsStat {
        path: path.clone(),
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        version: fs_version(&metadata),
    };
    Ok(FsWrite {
        path,
        bytes_written,
        version: version_from_metadata(metadata),
        stat: Some(stat),
    })
}

pub(crate) async fn write_range(
    state: &AppState,
    path: String,
    offset: u64,
    data: Vec<u8>,
    precondition: Option<FsPrecondition>,
) -> Result<FsWrite, Status> {
    validate_write_chunk(data.len())?;
    checked_file_end(offset, data.len(), "write range")?;
    authorize_fs_action(state, "write-range", &path, "write", &path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let (full_path, parent_exists) = resolve_write_target(state, "write-range", &path, &path)?;
    check_precondition(&full_path, precondition.as_ref())?;
    if let Some(parent) = full_path.parent().filter(|_| !parent_exists) {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(status_from_io_error)?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(status_from_io_error)?;
    file.write_all(&data).await.map_err(status_from_io_error)?;
    file.flush().await.map_err(status_from_io_error)?;
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    let stat = FsStat {
        path: path.clone(),
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        version: fs_version(&metadata),
    };
    drop(_mutation);
    record_audit(state, "write-range", &path, true, "allowed");
    Ok(FsWrite {
        path,
        bytes_written: data.len() as u64,
        version: version_from_metadata(metadata),
        stat: Some(stat),
    })
}

pub(crate) async fn truncate(
    state: &AppState,
    path: String,
    size: u64,
    precondition: Option<FsPrecondition>,
) -> Result<FsStat, Status> {
    if size > MAX_FS_FILE_BYTES {
        return Err(Status::invalid_argument(format!(
            "truncate size exceeds maximum fs object size of {} bytes",
            MAX_FS_FILE_BYTES
        )));
    }
    authorize_fs_action(state, "truncate", &path, "write", &path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let (full_path, parent_exists) = resolve_write_target(state, "truncate", &path, &path)?;
    check_precondition(&full_path, precondition.as_ref())?;
    if let Some(parent) = full_path.parent().filter(|_| !parent_exists) {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(status_from_io_error)?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&full_path)
        .await
        .map_err(status_from_io_error)?;
    file.set_len(size).await.map_err(status_from_io_error)?;
    record_audit(state, "truncate", &path, true, "allowed");
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        version: version_from_metadata(metadata),
    })
}

pub(crate) async fn mkdir(state: &AppState, path: String) -> Result<FsStat, Status> {
    authorize_fs_action(state, "mkdir", &path, "write", &path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let full_path = resolve_create_path(state, "mkdir", &path, &path)?;
    tokio::fs::create_dir_all(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "mkdir", &path, true, "allowed");
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        version: version_from_metadata(metadata),
    })
}

pub(crate) async fn delete(
    state: &AppState,
    path: String,
    precondition: Option<FsPrecondition>,
) -> Result<String, Status> {
    authorize_fs_action(state, "delete", &path, "delete", &path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let full_path = resolve_existing_leaf_path(state, "delete", &path, &path)?;
    check_precondition(&full_path, precondition.as_ref())?;
    let metadata = tokio::fs::symlink_metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    if metadata.is_dir() {
        tokio::fs::remove_dir(&full_path)
            .await
            .map_err(status_from_io_error)?;
    } else {
        tokio::fs::remove_file(&full_path)
            .await
            .map_err(status_from_io_error)?;
    }
    record_audit(state, "delete", &path, true, "allowed");
    Ok(path)
}

pub(crate) async fn rename(
    state: &AppState,
    from_path: &str,
    to_path: &str,
    from_precondition: Option<FsPrecondition>,
    to_precondition: Option<FsPrecondition>,
) -> Result<(), Status> {
    let resource = format!("{from_path} -> {to_path}");
    authorize_fs_action(state, "rename", &resource, "delete", from_path)?;
    authorize_fs_action(state, "rename", &resource, "write", to_path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let from_full_path = resolve_existing_leaf_path(state, "rename", &resource, from_path)?;
    let to_full_path = resolve_write_path(state, "rename", &resource, to_path)?;
    check_precondition(&from_full_path, from_precondition.as_ref())?;
    check_precondition(&to_full_path, to_precondition.as_ref())?;
    tokio::fs::rename(&from_full_path, &to_full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "rename", &resource, true, "allowed");
    Ok(())
}

pub(crate) async fn copy(
    state: &AppState,
    from_path: &str,
    to_path: &str,
    from_precondition: Option<FsPrecondition>,
    to_precondition: Option<FsPrecondition>,
) -> Result<(u64, String), Status> {
    let resource = format!("{from_path} -> {to_path}");
    authorize_fs_action(state, "copy", &resource, "read", from_path)?;
    authorize_fs_action(state, "copy", &resource, "write", to_path)?;
    let _mutation = MUTATION_LOCK.lock().await;
    let from_full_path = resolve_existing_path(state, "copy", &resource, from_path)?;
    let to_full_path = resolve_write_path(state, "copy", &resource, to_path)?;
    check_precondition(&from_full_path, from_precondition.as_ref())?;
    check_precondition(&to_full_path, to_precondition.as_ref())?;
    let metadata = tokio::fs::metadata(&from_full_path)
        .await
        .map_err(status_from_io_error)?;
    if !metadata.is_file() {
        record_audit(state, "copy", &resource, false, "copy source is not a file");
        return Err(Status::failed_precondition("copy source is not a file"));
    }
    let bytes_copied = tokio::fs::copy(&from_full_path, &to_full_path)
        .await
        .map_err(status_from_io_error)?;
    let metadata = tokio::fs::metadata(&to_full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "copy", &resource, true, "allowed");
    Ok((bytes_copied, version_from_metadata(metadata)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writable_state(workspace: PathBuf) -> AppState {
        let mut policy = crate::defaults::default_policy();
        policy.fs.mounts = vec![operon_core::FsMountPolicy {
            name: "workspace".into(),
            path: "/".into(),
            permissions: operon_core::FsPermissions {
                read: true,
                write: true,
                delete: true,
            },
        }];
        crate::daemon_state::test_state(policy, workspace)
    }

    fn target(path: &str, expected_version: Option<String>) -> WriteFileRequest {
        WriteFileRequest {
            payload: Some(write_file_request::Payload::Target(
                operon_protocol::runtime::v1::WriteFileTarget {
                    path: path.into(),
                    expected_version,
                    require_absent: false,
                    precondition: None,
                },
            )),
        }
    }

    fn chunk(data: &[u8]) -> WriteFileRequest {
        WriteFileRequest {
            payload: Some(write_file_request::Payload::Chunk(FileChunk {
                data: data.into(),
            })),
        }
    }

    #[tokio::test]
    async fn write_flush_and_commit_failures_preserve_target_and_cleanup() {
        for exists in [false, true] {
            for failure in [
                StreamIoFailure::Write,
                StreamIoFailure::Flush,
                StreamIoFailure::Commit,
            ] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("file");
                if exists {
                    std::fs::write(&path, b"ORIGINAL").unwrap();
                }
                let state = writable_state(dir.path().into());
                let mut stream =
                    futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
                assert!(write_stream_impl(&state, &mut stream, Some(failure))
                    .await
                    .is_err());
                let audit = state.audit.lock().unwrap();
                let event = audit.back().unwrap();
                assert_eq!(event.action, "write-stream");
                assert!(!event.allowed);
                drop(audit);
                if exists {
                    assert_eq!(std::fs::read(&path).unwrap(), b"ORIGINAL");
                } else {
                    assert!(!path.exists());
                }
                assert_eq!(
                    std::fs::read_dir(dir.path()).unwrap().count(),
                    usize::from(exists)
                );
            }
        }
    }

    #[tokio::test]
    async fn stream_require_absent_rechecked_at_commit() {
        let dir = tempfile::tempdir().unwrap();
        let state = writable_state(dir.path().into());
        let mut request = target("/file", None);
        if let Some(write_file_request::Payload::Target(target)) = &mut request.payload {
            target.require_absent = true;
        }
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(Ok(request)).await.unwrap();
        tx.send(Ok(chunk(b"NEW"))).await.unwrap();
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            let mut stream = futures_util::stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|v| (v, rx))
            })
            .boxed();
            write_stream(&task_state, &mut stream).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if std::fs::read_dir(dir.path()).unwrap().any(|e| {
                    e.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".operon-write-")
                }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        write_range(&state, "/file".into(), 0, b"CONCURRENT".to_vec(), None)
            .await
            .unwrap();
        drop(tx);
        assert_eq!(
            task.await.unwrap().unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            std::fs::read(dir.path().join("file")).unwrap(),
            b"CONCURRENT"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stream_replaces_leaf_symlink_target_without_replacing_link() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"ORIGINAL").unwrap();
        std::os::unix::fs::symlink("file", dir.path().join("link")).unwrap();
        let state = writable_state(dir.path().into());
        let mut stream = futures_util::stream::iter([Ok(target("/link", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), b"NEW");
        assert_eq!(
            std::fs::read_link(dir.path().join("link")).unwrap(),
            std::path::Path::new("file")
        );
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn replacement_preserves_posix_acl_and_removes_inherited_acl_when_absent() {
        use std::os::fd::AsRawFd;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"ORIGINAL").unwrap();
        let mut acl = 2u32.to_le_bytes().to_vec();
        // Linux POSIX ACL xattr: owner, named user, group, mask, other.
        for (tag, permissions, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, 12345),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permissions.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        let set_acl = |file: &std::fs::File, name: &std::ffi::CStr| {
            assert_eq!(
                unsafe {
                    libc::fsetxattr(
                        file.as_raw_fd(),
                        name.as_ptr(),
                        acl.as_ptr().cast(),
                        acl.len(),
                        0,
                    )
                },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
        };
        let read_acl = |path: &std::path::Path| {
            let file = std::fs::File::open(path).unwrap();
            let mut result = vec![0u8; 1024];
            let length = unsafe {
                libc::fgetxattr(
                    file.as_raw_fd(),
                    c"system.posix_acl_access".as_ptr(),
                    result.as_mut_ptr().cast(),
                    result.len(),
                )
            };
            if length < 0 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ENODATA)
                );
                return None;
            }
            result.truncate(length as usize);
            Some(result)
        };
        set_acl(
            &std::fs::File::open(&path).unwrap(),
            c"system.posix_acl_access",
        );
        let state = writable_state(dir.path().into());
        let mut stream = futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(read_acl(&path), Some(acl.clone()));

        // The old plain file has no ACL, but a new staged file would inherit
        // this directory default ACL. Replacement must remove that inheritance.
        std::fs::write(dir.path().join("plain"), b"ORIGINAL").unwrap();
        assert_eq!(read_acl(&dir.path().join("plain")), None);
        set_acl(
            &std::fs::File::open(dir.path()).unwrap(),
            c"system.posix_acl_default",
        );
        let mut stream =
            futures_util::stream::iter([Ok(target("/plain", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(read_acl(&dir.path().join("plain")), None);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn replacement_preserves_macos_extended_acl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"ORIGINAL").unwrap();
        assert!(std::process::Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(&path)
            .status()
            .unwrap()
            .success());
        let acl = || {
            let output = std::process::Command::new("ls")
                .arg("-le")
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let before = acl();
        assert!(before.contains("everyone allow read"));
        let state = writable_state(dir.path().into());
        let mut stream = futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(acl(), before);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn replacement_preserves_windows_dacl_and_sharing_failure_preserves_target() {
        use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt};
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW,
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
                    SetNamedSecurityInfoW, SE_FILE_OBJECT,
                },
                GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION,
                PROTECTED_DACL_SECURITY_INFORMATION,
            },
        };
        struct LocalBuffer(*mut std::ffi::c_void);
        impl Drop for LocalBuffer {
            fn drop(&mut self) {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"ORIGINAL").unwrap();
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // A protected owner-rights ACL is intentionally distinct from the
        // staging file's inherited ACL. Use Win32 directly: PowerShell module
        // autoload depends on the runner's PSModulePath/shell-version pairing.
        let sddl = "D:P(A;;FA;;;OW)"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut descriptor = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let descriptor = LocalBuffer(descriptor);
        let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted)
            },
            0
        );
        assert_ne!(present, 0);
        assert_eq!(
            unsafe {
                SetNamedSecurityInfoW(
                    wide.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null(),
                )
            },
            0
        );
        let acl = || {
            let mut descriptor = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    GetNamedSecurityInfoW(
                        wide.as_ptr(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut descriptor,
                    )
                },
                0
            );
            let descriptor = LocalBuffer(descriptor);
            let mut string = std::ptr::null_mut();
            assert_ne!(
                unsafe {
                    ConvertSecurityDescriptorToStringSecurityDescriptorW(
                        descriptor.0,
                        1,
                        DACL_SECURITY_INFORMATION,
                        &mut string,
                        std::ptr::null_mut(),
                    )
                },
                0
            );
            let _string = LocalBuffer(string.cast());
            let mut length = 0;
            while unsafe { *string.add(length) } != 0 {
                length += 1;
            }
            String::from_utf16(unsafe { std::slice::from_raw_parts(string, length) }).unwrap()
        };
        let before = acl();
        assert!(before.contains("OW"));
        let state = writable_state(dir.path().into());
        // Allow reads/writes but deny delete-sharing required by ReplaceFile.
        let old = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let mut stream = futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
        assert!(write_stream(&state, &mut stream).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"ORIGINAL");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        drop(old);
        let mut stream = futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"NEW");
        assert_eq!(acl(), before);
    }

    #[tokio::test]
    async fn failed_stream_preserves_existing_and_absent_targets() {
        let dir = tempfile::tempdir().unwrap();
        let state = writable_state(dir.path().into());
        for exists in [true, false] {
            let path = if exists { "/existing" } else { "/absent" };
            if exists {
                std::fs::write(dir.path().join("existing"), b"ORIGINAL").unwrap();
            }
            for failure in [
                Ok(WriteFileRequest { payload: None }),
                Err(Status::cancelled("test")),
                Ok(chunk(&vec![0; MAX_FS_WRITE_CHUNK_BYTES + 1])),
            ] {
                let mut stream = futures_util::stream::iter([
                    Ok(target(path, None)),
                    Ok(chunk(b"NEW")),
                    failure,
                ]);
                assert!(write_stream(&state, &mut stream).await.is_err());
                if exists {
                    assert_eq!(
                        std::fs::read(dir.path().join("existing")).unwrap(),
                        b"ORIGINAL"
                    );
                } else {
                    assert!(!dir.path().join("absent").exists());
                }
                assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".operon-write-")));
            }
        }
    }

    #[tokio::test]
    async fn dropped_stream_cleans_staging_and_empty_stream_commits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"ORIGINAL").unwrap();
        let state = writable_state(dir.path().into());
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(Ok(target("/file", None))).await.unwrap();
        tx.send(Ok(chunk(b"NEW"))).await.unwrap();
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            let mut stream = futures_util::stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|v| (v, rx))
            })
            .boxed();
            write_stream(&task_state, &mut stream).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if std::fs::read_dir(dir.path()).unwrap().any(|e| {
                    e.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".operon-write-")
                }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        drop(tx);
        assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), b"ORIGINAL");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        let mut stream = futures_util::stream::iter([Ok(target("/file", None))]);
        let result = write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(result.bytes_written, 0);
        assert_eq!(result.stat.unwrap().size, 0);
        assert!(std::fs::read(dir.path().join("file")).unwrap().is_empty());
    }

    #[tokio::test]
    async fn stream_rechecks_version_after_concurrent_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let state = writable_state(dir.path().into());
        std::fs::write(dir.path().join("file"), b"ORIGINAL").unwrap();
        let version = fs_version(&std::fs::metadata(dir.path().join("file")).unwrap());
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(Ok(target("/file", Some(version)))).await.unwrap();
        tx.send(Ok(chunk(b"NEW"))).await.unwrap();
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            let mut stream = futures_util::stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|v| (v, rx))
            })
            .boxed();
            write_stream(&task_state, &mut stream).await
        });
        for _ in 0..1000 {
            if std::fs::read_dir(dir.path()).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".operon-write-")
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        // A successful range mutation is coordinated with stream commit, but
        // never held behind an unfinished client stream.
        write_range(&state, "/file".into(), 0, b"CONCURRENT".to_vec(), None)
            .await
            .unwrap();
        drop(tx);
        assert_eq!(
            task.await.unwrap().unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            std::fs::read(dir.path().join("file")).unwrap(),
            b"CONCURRENT"
        );
    }

    #[tokio::test]
    async fn replacement_preserves_mode_and_old_handles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"ORIGINAL").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
            std::fs::hard_link(&path, dir.path().join("link")).unwrap();
        }
        #[cfg(unix)]
        let mut old = std::fs::File::open(&path).unwrap();
        #[cfg(not(unix))]
        let _old = std::fs::File::open(&path).unwrap();
        let state = writable_state(dir.path().into());
        let mut stream = futures_util::stream::iter([Ok(target("/file", None)), Ok(chunk(b"NEW"))]);
        write_stream(&state, &mut stream).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"NEW");
        #[cfg(unix)]
        {
            use std::io::Read;
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
            let mut data = Vec::new();
            old.read_to_end(&mut data).unwrap();
            assert_eq!(data, b"ORIGINAL");
            assert_eq!(std::fs::read(dir.path().join("link")).unwrap(), b"ORIGINAL");
        }
    }

    #[tokio::test]
    async fn range_fill_handles_small_reads_eof_and_zero_size() {
        let (mut writer, mut reader) = tokio::io::duplex(3);
        let send = tokio::spawn(async move {
            writer.write_all(b"0123456789").await.unwrap();
        });
        let mut data = [0; 16];
        assert_eq!(fill_range(&mut reader, &mut data).await.unwrap(), 10);
        assert_eq!(&data[..10], b"0123456789");
        assert_eq!(fill_range(&mut reader, &mut []).await.unwrap(), 0);
        send.await.unwrap();
    }

    #[tokio::test]
    async fn range_fill_exceeds_tokio_file_buffer_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large");
        let source = (0..9 * 1024 * 1024)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        std::fs::write(&path, &source).unwrap();
        let mut file = tokio::fs::File::open(path).await.unwrap();
        file.seek(std::io::SeekFrom::Start(37)).await.unwrap();
        let mut data = vec![0; 8 * 1024 * 1024];
        assert_eq!(fill_range(&mut file, &mut data).await.unwrap(), data.len());
        assert_eq!(data, source[37..37 + data.len()]);
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "operond-fs-service-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }

    #[test]
    fn fs_version_changes_when_file_metadata_changes() {
        let path = temp_path("version");
        std::fs::write(&path, "old").expect("write old");
        let first = fs_version(&std::fs::metadata(&path).expect("first metadata"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        std::fs::write(&path, "new-content").expect("write new");
        let second = fs_version(&std::fs::metadata(&path).expect("second metadata"));

        assert_ne!(first, second);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn check_precondition_rejects_mismatched_expected_version() {
        let path = temp_path("precondition");
        std::fs::write(&path, "data").expect("write");
        let error = check_precondition(
            &path,
            Some(&FsPrecondition {
                expected_version: Some("stale-version".to_string()),
                require_absent: false,
            }),
        )
        .expect_err("stale version should fail");

        assert_eq!(error.code(), tonic::Code::FailedPrecondition);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn check_precondition_allows_require_absent_for_missing_target() {
        let path = temp_path("absent");
        check_precondition(
            &path,
            Some(&FsPrecondition {
                expected_version: None,
                require_absent: true,
            }),
        )
        .expect("missing path should satisfy require_absent");
    }
}
