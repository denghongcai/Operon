use operon_core::{FsEntry, FsList, FsStat, FsWrite};
use operon_fs::{
    authorize_fs, join_virtual_path, resolve_create_workspace_path,
    resolve_existing_workspace_leaf_path, resolve_existing_workspace_path,
    resolve_write_workspace_path,
};
use operon_protocol::runtime::v1::FileChunk;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tonic::Status;

use crate::{
    grpc_status::{status_from_error, status_from_io_error},
    record_audit, AppState, MAX_FS_FILE_BYTES, MAX_FS_WRITE_CHUNK_BYTES,
};

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

pub(crate) async fn stat(state: &AppState, path: String) -> Result<FsStat, Status> {
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "stat", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "stat", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "stat", &path, true, "allowed");
    Ok(FsStat {
        path,
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
    })
}

pub(crate) async fn list(state: &AppState, path: String) -> Result<FsList, Status> {
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "list", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "list", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    record_audit(state, "list", &path, true, "allowed");
    Ok(FsList { path, entries })
}

pub(crate) async fn read_range(
    state: &AppState,
    path: String,
    offset: u64,
    size: u32,
) -> Result<FileChunk, Status> {
    validate_read_range_size(size)?;
    checked_file_end(offset, size as usize, "read range")?;
    if let Err(error) = authorize_fs(&state.policy, "read", &path) {
        record_audit(state, "read-range", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "read-range", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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

pub(crate) async fn write_range(
    state: &AppState,
    path: String,
    offset: u64,
    data: Vec<u8>,
) -> Result<FsWrite, Status> {
    validate_write_chunk(data.len())?;
    checked_file_end(offset, data.len(), "write range")?;
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "write-range", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_write_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "write-range", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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
    record_audit(state, "write-range", &path, true, "allowed");
    Ok(FsWrite {
        path,
        bytes_written: data.len() as u64,
    })
}

pub(crate) async fn truncate(state: &AppState, path: String, size: u64) -> Result<FsStat, Status> {
    if size > MAX_FS_FILE_BYTES {
        return Err(Status::invalid_argument(format!(
            "truncate size exceeds maximum fs object size of {} bytes",
            MAX_FS_FILE_BYTES
        )));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "truncate", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_write_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "truncate", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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
    })
}

pub(crate) async fn mkdir(state: &AppState, path: String) -> Result<FsStat, Status> {
    if let Err(error) = authorize_fs(&state.policy, "write", &path) {
        record_audit(state, "mkdir", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_create_workspace_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "mkdir", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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
    })
}

pub(crate) async fn delete(state: &AppState, path: String) -> Result<String, Status> {
    if let Err(error) = authorize_fs(&state.policy, "delete", &path) {
        record_audit(state, "delete", &path, false, &error.1);
        return Err(status_from_error(error));
    }
    let full_path = match resolve_existing_workspace_leaf_path(&state.workspace, &path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "delete", &path, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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

pub(crate) async fn rename(state: &AppState, from_path: &str, to_path: &str) -> Result<(), Status> {
    let resource = format!("{from_path} -> {to_path}");
    if let Err(error) = authorize_fs(&state.policy, "delete", from_path) {
        record_audit(state, "rename", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", to_path) {
        record_audit(state, "rename", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    let from_full_path = match resolve_existing_workspace_leaf_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "rename", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_write_workspace_path(&state.workspace, to_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "rename", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    tokio::fs::rename(&from_full_path, &to_full_path)
        .await
        .map_err(status_from_io_error)?;
    record_audit(state, "rename", &resource, true, "allowed");
    Ok(())
}

pub(crate) async fn copy(state: &AppState, from_path: &str, to_path: &str) -> Result<u64, Status> {
    let resource = format!("{from_path} -> {to_path}");
    if let Err(error) = authorize_fs(&state.policy, "read", from_path) {
        record_audit(state, "copy", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    if let Err(error) = authorize_fs(&state.policy, "write", to_path) {
        record_audit(state, "copy", &resource, false, &error.1);
        return Err(status_from_error(error));
    }
    let from_full_path = match resolve_existing_workspace_path(&state.workspace, from_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "copy", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
    let to_full_path = match resolve_write_workspace_path(&state.workspace, to_path) {
        Ok(path) => path,
        Err(error) => {
            record_audit(state, "copy", &resource, false, &error.1);
            return Err(status_from_error(error));
        }
    };
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
    record_audit(state, "copy", &resource, true, "allowed");
    Ok(bytes_copied)
}
