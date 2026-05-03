use std::{path::PathBuf, pin::Pin};

use futures_util::{Stream, StreamExt};
use operon_core::{FsEntry, FsList, FsPrecondition, FsStat, FsWrite};
use operon_fs::{
    authorize_fs_decision, join_virtual_path, resolve_create_workspace_path,
    resolve_existing_workspace_leaf_path, resolve_existing_workspace_path,
    resolve_write_workspace_path,
};
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
    resolve_existing_workspace_path(&state.workspace, path).map_err(|error| {
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
    resolve_existing_workspace_leaf_path(&state.workspace, path).map_err(|error| {
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
    resolve_write_workspace_path(&state.workspace, path).map_err(|error| {
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
    resolve_create_workspace_path(&state.workspace, path).map_err(|error| {
        record_audit(state, audit_action, audit_resource, false, &error.1);
        status_from_error(error)
    })
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
    let stream = ReaderStream::new(file).map(|chunk| {
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
    let bytes_read = file.read(&mut data).await.map_err(status_from_io_error)?;
    data.truncate(bytes_read);
    record_audit(state, "read-range", &path, true, "allowed");
    Ok(FileChunk { data })
}

pub(crate) async fn write_stream(
    state: &AppState,
    stream: &mut tonic::Streaming<WriteFileRequest>,
) -> Result<FsWrite, Status> {
    let mut path = None;
    let mut file = None;
    let mut bytes_written = 0_u64;

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
                let full_path =
                    resolve_write_path(state, "write-stream", &target.path, &target.path)?;
                let precondition = grpc_precondition(
                    target.precondition,
                    target.expected_version,
                    target.require_absent,
                );
                check_precondition(&full_path, precondition.as_ref())?;
                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(status_from_io_error)?;
                }
                file = Some(
                    tokio::fs::File::create(&full_path)
                        .await
                        .map_err(status_from_io_error)?,
                );
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
    let full_path = resolve_existing_path(state, "write-stream", &path, &path)?;
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "write-stream", &path, true, "allowed");
    Ok(FsWrite {
        path,
        bytes_written,
        version: version_from_metadata(metadata),
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
    let full_path = resolve_write_path(state, "write-range", &path, &path)?;
    check_precondition(&full_path, precondition.as_ref())?;
    if let Some(parent) = full_path.parent() {
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
    record_audit(state, "write-range", &path, true, "allowed");
    Ok(FsWrite {
        path,
        bytes_written: data.len() as u64,
        version: version_from_metadata(metadata),
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
    let full_path = resolve_write_path(state, "truncate", &path, &path)?;
    check_precondition(&full_path, precondition.as_ref())?;
    if let Some(parent) = full_path.parent() {
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
