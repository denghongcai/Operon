#![cfg(windows)]

use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    time::Duration,
};

use anyhow::Context;
use operon_core::FsStat;
use operon_network::NodeEndpoint;
use winfsp_wrs::{
    u16cstr, u16str, CleanupFlags, CreateFileInfo, CreateOptions, DirInfo, FileAccessRights,
    FileAttributes, FileInfo, FileSystem, FileSystemInterface, OperationGuardStrategy,
    PSecurityDescriptor, Params, SecurityDescriptor, U16CStr, U16CString, VolumeInfo, VolumeParams,
    WriteMode, NTSTATUS, STATUS_ACCESS_DENIED, STATUS_FILE_IS_A_DIRECTORY,
    STATUS_INVALID_PARAMETER, STATUS_NOT_A_DIRECTORY, STATUS_NOT_IMPLEMENTED,
    STATUS_OBJECT_NAME_NOT_FOUND,
};

use crate::{
    mount_core::{
        classify_mount_error, join_remote_child, normalize_remote_path, MountAdapterCore,
        MountErrorKind, RemoteFs,
    },
    remote_client::GrpcRemoteFs,
};

#[derive(Debug, Clone)]
pub struct MountOptions {
    pub endpoint: NodeEndpoint,
    pub remote_path: String,
    pub mount_point: PathBuf,
}

pub struct MountSession {
    file_system: FileSystem,
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
        self.file_system.stop();
        Ok(())
    }
}

fn trace_mount_event(event: impl AsRef<str>, detail: impl AsRef<str>) {
    if std::env::var_os("OPERON_MOUNT_TRACE").is_some() {
        eprintln!(
            "operon-mount windows {}: {}",
            event.as_ref(),
            detail.as_ref()
        );
    }
}

pub fn spawn_mount(options: MountOptions) -> anyhow::Result<MountSession> {
    winfsp_wrs::init().map_err(|error| anyhow::anyhow!("failed to initialize WinFsp: {error}"))?;

    let remote_root = normalize_remote_path(&options.remote_path)?;
    let remote_fs = Arc::new(GrpcRemoteFs::connect(options.endpoint)?);
    let root = remote_fs.stat(&remote_root)?;
    if !root.is_dir {
        anyhow::bail!("mount root `{remote_root}` is not a directory");
    }

    let mount_point = options.mount_point.display().to_string();
    let mount_point = U16CString::from_str(&mount_point).with_context(|| {
        format!(
            "mount point `{}` is not valid UTF-16",
            options.mount_point.display()
        )
    })?;
    let context = OperonWinFspFs::new(MountAdapterCore::new(remote_fs), remote_root)?;
    trace_mount_event("start", options.mount_point.display().to_string());
    let file_system = FileSystem::start(
        Params {
            volume_params: volume_params(),
            guard_strategy: OperationGuardStrategy::Fine,
        },
        Some(mount_point.as_ucstr()),
        context,
    )
    .map_err(|status| anyhow::anyhow!("failed to start WinFsp mount: NTSTATUS {status:#x}"))?;

    Ok(MountSession { file_system })
}

struct OperonWinFspFs {
    core: MountAdapterCore,
    root_path: String,
    security: SecurityDescriptor,
}

impl OperonWinFspFs {
    fn new(core: MountAdapterCore, root_path: String) -> anyhow::Result<Self> {
        let security = SecurityDescriptor::from_wstr(u16cstr!(
            "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
        ))
        .map_err(|error| anyhow::anyhow!(error))?;
        Ok(Self {
            core,
            root_path,
            security,
        })
    }

    fn path_for_name(&self, name: &U16CStr) -> Result<String, NTSTATUS> {
        windows_name_to_remote_path(&self.root_path, name).map_err(|_| STATUS_INVALID_PARAMETER)
    }

    fn stat_for_name(&self, name: &U16CStr) -> Result<FsStat, NTSTATUS> {
        let path = self.path_for_name(name)?;
        self.core.stat(&path).map_err(ntstatus_for_error)
    }

    fn file_context_for_stat(stat: &FsStat) -> Arc<WindowsFileContext> {
        Arc::new(WindowsFileContext {
            path: stat.path.clone(),
            is_dir: stat.is_dir,
        })
    }
}

struct WindowsFileContext {
    path: String,
    is_dir: bool,
}

impl FileSystemInterface for OperonWinFspFs {
    type FileContext = Arc<WindowsFileContext>;

    const GET_VOLUME_INFO_DEFINED: bool = true;
    const GET_SECURITY_BY_NAME_DEFINED: bool = true;
    const CREATE_DEFINED: bool = true;
    const OPEN_DEFINED: bool = true;
    const CLEANUP_DEFINED: bool = true;
    const CLOSE_DEFINED: bool = true;
    const READ_DEFINED: bool = true;
    const WRITE_DEFINED: bool = true;
    const FLUSH_DEFINED: bool = true;
    const GET_FILE_INFO_DEFINED: bool = true;
    const SET_FILE_SIZE_DEFINED: bool = true;
    const CAN_DELETE_DEFINED: bool = true;
    const RENAME_DEFINED: bool = true;
    const GET_SECURITY_DEFINED: bool = true;
    const READ_DIRECTORY_DEFINED: bool = true;
    const GET_DIR_INFO_BY_NAME_DEFINED: bool = true;
    const DISPATCHER_STOPPED_DEFINED: bool = true;

    fn get_volume_info(&self) -> Result<VolumeInfo, NTSTATUS> {
        trace_mount_event("get_volume_info", self.root_path.clone());
        VolumeInfo::new(1 << 40, 1 << 39, u16str!("Operon")).map_err(|_| STATUS_INVALID_PARAMETER)
    }

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _find_reparse_point: impl Fn() -> Option<FileAttributes>,
    ) -> Result<(FileAttributes, PSecurityDescriptor, bool), NTSTATUS> {
        let stat = self.stat_for_name(file_name)?;
        trace_mount_event("get_security_by_name", stat.path.clone());
        Ok((attributes_for_stat(&stat), self.security.as_ptr(), false))
    }

    fn create(
        &self,
        file_name: &U16CStr,
        create_file_info: CreateFileInfo,
        _security_descriptor: SecurityDescriptor,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        let path = self.path_for_name(file_name)?;
        trace_mount_event("create", path.clone());
        let stat = if create_file_info
            .create_options
            .is(CreateOptions::FILE_DIRECTORY_FILE)
        {
            self.core.mkdir(&path)
        } else {
            self.core.create_file(&path)
        }
        .map_err(ntstatus_for_error)?;

        Ok((
            Self::file_context_for_stat(&stat),
            file_info_for_stat(&stat),
        ))
    }

    fn open(
        &self,
        file_name: &U16CStr,
        create_options: CreateOptions,
        _granted_access: FileAccessRights,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        let stat = self.stat_for_name(file_name)?;
        trace_mount_event("open", stat.path.clone());
        if stat.is_dir && create_options.is(CreateOptions::FILE_NON_DIRECTORY_FILE) {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        if stat.is_file && create_options.is(CreateOptions::FILE_DIRECTORY_FILE) {
            return Err(STATUS_NOT_A_DIRECTORY);
        }

        Ok((
            Self::file_context_for_stat(&stat),
            file_info_for_stat(&stat),
        ))
    }

    fn cleanup(
        &self,
        file_context: Self::FileContext,
        _file_name: Option<&U16CStr>,
        flags: CleanupFlags,
    ) {
        if flags.is(CleanupFlags::DELETE) {
            let _ = self.core.delete(&file_context.path);
        }
    }

    fn close(&self, _file_context: Self::FileContext) {}

    fn read(
        &self,
        file_context: Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<usize, NTSTATUS> {
        if file_context.is_dir {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        let data = self
            .core
            .read_file(
                &file_context.path,
                offset,
                buffer.len().min(u32::MAX as usize) as u32,
            )
            .map_err(ntstatus_for_error)?;
        let copied = data.len().min(buffer.len());
        buffer[..copied].copy_from_slice(&data[..copied]);
        Ok(copied)
    }

    fn write(
        &self,
        file_context: Self::FileContext,
        buffer: &[u8],
        mode: WriteMode,
    ) -> Result<(usize, FileInfo), NTSTATUS> {
        if file_context.is_dir {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        let offset = match mode {
            WriteMode::Normal { offset } | WriteMode::ConstrainedIO { offset } => offset,
            WriteMode::WriteToEOF => {
                self.core
                    .stat(&file_context.path)
                    .map_err(ntstatus_for_error)?
                    .size
            }
        };
        let written = self
            .core
            .write_file(&file_context.path, offset, buffer)
            .map_err(ntstatus_for_error)?;
        let stat = self
            .core
            .stat(&file_context.path)
            .map_err(ntstatus_for_error)?;
        Ok((
            written.min(usize::MAX as u64) as usize,
            file_info_for_stat(&stat),
        ))
    }

    fn flush(&self, file_context: Self::FileContext) -> Result<FileInfo, NTSTATUS> {
        let stat = self
            .core
            .stat(&file_context.path)
            .map_err(ntstatus_for_error)?;
        Ok(file_info_for_stat(&stat))
    }

    fn get_file_info(&self, file_context: Self::FileContext) -> Result<FileInfo, NTSTATUS> {
        let stat = self
            .core
            .stat(&file_context.path)
            .map_err(ntstatus_for_error)?;
        Ok(file_info_for_stat(&stat))
    }

    fn set_file_size(
        &self,
        file_context: Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
    ) -> Result<FileInfo, NTSTATUS> {
        if file_context.is_dir {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        let stat = self
            .core
            .truncate(&file_context.path, new_size)
            .map_err(ntstatus_for_error)?;
        Ok(file_info_for_stat(&stat))
    }

    fn can_delete(
        &self,
        _file_context: Self::FileContext,
        _file_name: &U16CStr,
    ) -> Result<(), NTSTATUS> {
        Ok(())
    }

    fn rename(
        &self,
        file_context: Self::FileContext,
        _file_name: &U16CStr,
        new_file_name: &U16CStr,
        _replace_if_exists: bool,
    ) -> Result<(), NTSTATUS> {
        let new_path = self.path_for_name(new_file_name)?;
        self.core
            .rename(&file_context.path, &new_path)
            .map_err(ntstatus_for_error)
    }

    fn get_security(
        &self,
        _file_context: Self::FileContext,
    ) -> Result<PSecurityDescriptor, NTSTATUS> {
        Ok(self.security.as_ptr())
    }

    fn read_directory(
        &self,
        file_context: Self::FileContext,
        marker: Option<&U16CStr>,
        mut add_dir_info: impl FnMut(DirInfo) -> bool,
    ) -> Result<(), NTSTATUS> {
        if !file_context.is_dir {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        let marker = marker.map(|value| value.to_string_lossy());
        trace_mount_event(
            "read_directory",
            format!("path={} marker={marker:?}", file_context.path),
        );
        let mut seen_marker = marker.is_none();
        let entries = self
            .core
            .list_dir(&file_context.path)
            .map_err(ntstatus_for_error)?;
        for entry in entries {
            trace_mount_event("read_directory_entry", entry.name.clone());
            if !seen_marker {
                seen_marker = marker.as_deref() == Some(entry.name.as_str());
                continue;
            }
            if !add_dir_info(DirInfo::from_str(
                file_info_for_stat(&entry.stat),
                &entry.name,
            )) {
                break;
            }
        }
        Ok(())
    }

    fn get_dir_info_by_name(
        &self,
        file_context: Self::FileContext,
        file_name: &U16CStr,
    ) -> Result<FileInfo, NTSTATUS> {
        if !file_context.is_dir {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        let child = file_name.to_string_lossy();
        trace_mount_event(
            "get_dir_info_by_name",
            format!("parent={} child={child}", file_context.path),
        );
        let path =
            join_remote_child(&file_context.path, &child).map_err(|_| STATUS_INVALID_PARAMETER)?;
        let stat = self.core.stat(&path).map_err(ntstatus_for_error)?;
        Ok(file_info_for_stat(&stat))
    }

    fn dispatcher_stopped(&self, normally: bool) {
        trace_mount_event("dispatcher_stopped", format!("normally={normally}"));
    }
}

fn volume_params() -> VolumeParams {
    let mut params = VolumeParams::default();
    params
        .set_case_sensitive_search(true)
        .set_case_preserved_names(true)
        .set_unicode_on_disk(true)
        .set_persistent_acls(false)
        .set_read_only_volume(false)
        .set_max_component_length(255)
        .set_sector_size(4096)
        .set_sectors_per_allocation_unit(1)
        .set_volume_serial_number(0x0A0E_0001)
        .set_file_info_timeout(1000)
        .set_dir_info_timeout(1000)
        .set_volume_info_timeout(1000);
    let _ = params.set_file_system_name(u16cstr!("Operon"));
    params
}

fn windows_name_to_remote_path(root: &str, name: &U16CStr) -> anyhow::Result<String> {
    let root = normalize_remote_path(root)?;
    let name = name.to_string_lossy();
    let name = name.trim_matches('\\');
    if name.is_empty() {
        return Ok(root);
    }

    let mut path = root;
    for component in name.split('\\') {
        if component.contains(':') {
            anyhow::bail!("Windows alternate data streams are not supported");
        }
        path = join_remote_child(&path, component)?;
    }
    Ok(path)
}

fn attributes_for_stat(stat: &FsStat) -> FileAttributes {
    if stat.is_dir {
        FileAttributes::DIRECTORY
    } else {
        FileAttributes::NORMAL
    }
}

fn file_info_for_stat(stat: &FsStat) -> FileInfo {
    let mut info = FileInfo::default();
    let size = if stat.is_dir { 0 } else { stat.size };
    info.set_file_attributes(attributes_for_stat(stat))
        .set_allocation_size(size.div_ceil(4096) * 4096)
        .set_file_size(size)
        .set_time(winfsp_wrs::filetime_now())
        .set_hard_links(1);
    info
}

fn ntstatus_for_error(error: anyhow::Error) -> NTSTATUS {
    match classify_mount_error(&error) {
        MountErrorKind::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
        MountErrorKind::PermissionDenied => STATUS_ACCESS_DENIED,
        MountErrorKind::InvalidInput => STATUS_INVALID_PARAMETER,
        MountErrorKind::FailedPrecondition => STATUS_ACCESS_DENIED,
        MountErrorKind::Unknown => STATUS_NOT_IMPLEMENTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_names_map_under_remote_root() {
        assert_eq!(
            windows_name_to_remote_path("/workspace//root", u16cstr!("\\dir\\file.txt"))
                .expect("remote path"),
            "/workspace/root/dir/file.txt"
        );
        assert_eq!(
            windows_name_to_remote_path("/workspace", u16cstr!("\\")).expect("root"),
            "/workspace"
        );
    }

    #[test]
    fn windows_name_rejects_alternate_data_streams() {
        let error = windows_name_to_remote_path("/workspace", u16cstr!("\\file.txt:stream"))
            .expect_err("ads should be rejected");

        assert!(error.to_string().contains("alternate data streams"));
    }
}
