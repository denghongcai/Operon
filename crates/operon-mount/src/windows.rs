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
use windows_sys::Win32::{
    Foundation::{LocalFree, STATUS_BUFFER_OVERFLOW},
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION},
        GetSecurityDescriptorLength,
    },
};
use winfsp_wrs::{
    u16cstr, u16str, CreateOptions, U16CStr, U16CString, NTSTATUS, STATUS_FILE_IS_A_DIRECTORY,
    STATUS_INVALID_PARAMETER, STATUS_NOT_A_DIRECTORY, STATUS_SUCCESS,
};
use winfsp_wrs_sys::{
    FspCleanupDelete, FspFileSystemAddDirInfo, FspFileSystemCreate, FspFileSystemDelete,
    FspFileSystemRemoveMountPoint, FspFileSystemSetMountPoint,
    FspFileSystemSetOperationGuardStrategyF, FspFileSystemStartDispatcher,
    FspFileSystemStopDispatcher, FSP_FILE_SYSTEM, FSP_FILE_SYSTEM_INTERFACE,
    FSP_FILE_SYSTEM_OPERATION_GUARD_STRATEGY_FSP_FILE_SYSTEM_OPERATION_GUARD_STRATEGY_FINE,
    FSP_FSCTL_DIR_INFO, FSP_FSCTL_FILE_INFO, FSP_FSCTL_VOLUME_INFO, FSP_FSCTL_VOLUME_PARAMS,
    PSECURITY_DESCRIPTOR, PVOID, PWSTR, SIZE_T, UINT32, UINT64, ULONG,
};

use crate::{
    mount_core::{join_remote_child, normalize_remote_path, MountAdapterCore, RemoteFs},
    remote_client::GrpcRemoteFs,
    windows_file_info::{attributes_for_stat, file_info_for_stat},
    windows_path::windows_name_to_remote_path,
    windows_status::ntstatus_for_error,
};

#[derive(Debug, Clone)]
pub struct MountOptions {
    pub endpoint: NodeEndpoint,
    pub remote_path: String,
    pub mount_point: PathBuf,
}

pub struct MountSession {
    handles: Option<WinFspMountHandles>,
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

    pub fn unmount(mut self) -> anyhow::Result<()> {
        self.shutdown();
        Ok(())
    }

    fn shutdown(&mut self) {
        let Some(handles) = self.handles.take() else {
            return;
        };
        unsafe {
            FspFileSystemStopDispatcher(handles.file_system);
            FspFileSystemRemoveMountPoint(handles.file_system);
            FspFileSystemDelete(handles.file_system);
            drop(Box::from_raw(handles.context));
            drop(Box::from_raw(handles.interface));
        }
    }
}

impl Drop for MountSession {
    fn drop(&mut self) {
        self.shutdown();
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
    trace_mount_event("callback_flags", "direct_sys_interface=true create_ex=true");
    let handles = start_winfsp_mount(mount_point.as_ucstr(), context)
        .map_err(|status| anyhow::anyhow!("failed to start WinFsp mount: NTSTATUS {status:#x}"))?;

    Ok(MountSession {
        handles: Some(handles),
    })
}

struct OperonWinFspFs {
    core: MountAdapterCore,
    root_path: String,
    security: WindowsSecurityDescriptor,
}

impl OperonWinFspFs {
    fn new(core: MountAdapterCore, root_path: String) -> anyhow::Result<Self> {
        let security = WindowsSecurityDescriptor::from_sddl(u16cstr!(
            "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
        ))?;
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
}

struct WindowsFileContext {
    path: String,
    is_dir: bool,
}

struct WindowsSecurityDescriptor {
    bytes: Vec<u8>,
}

impl WindowsSecurityDescriptor {
    fn from_sddl(sddl: &U16CStr) -> anyhow::Result<Self> {
        let mut descriptor = std::ptr::null_mut();
        let mut reported_len = 0;
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION,
                &mut descriptor,
                &mut reported_len,
            )
        };
        if ok == 0 {
            anyhow::bail!("failed to create Windows security descriptor");
        }

        let len = unsafe { GetSecurityDescriptorLength(descriptor) as usize };
        let mut bytes = vec![0; len];
        unsafe {
            std::ptr::copy_nonoverlapping(descriptor.cast::<u8>(), bytes.as_mut_ptr(), len);
            LocalFree(descriptor.cast());
        }
        Ok(Self { bytes })
    }

    unsafe fn copy_to(
        &self,
        security_descriptor: PSECURITY_DESCRIPTOR,
        security_descriptor_size: *mut SIZE_T,
    ) -> NTSTATUS {
        if security_descriptor_size.is_null() {
            return STATUS_SUCCESS;
        }

        if self.bytes.len() as SIZE_T > security_descriptor_size.read() {
            security_descriptor_size.write(self.bytes.len() as SIZE_T);
            return STATUS_BUFFER_OVERFLOW;
        }

        security_descriptor_size.write(self.bytes.len() as SIZE_T);
        if !security_descriptor.is_null() {
            std::ptr::copy_nonoverlapping(
                self.bytes.as_ptr(),
                security_descriptor.cast::<u8>(),
                self.bytes.len(),
            );
        }
        STATUS_SUCCESS
    }
}

struct WinFspMountHandles {
    file_system: *mut FSP_FILE_SYSTEM,
    interface: *mut FSP_FILE_SYSTEM_INTERFACE,
    context: *mut OperonWinFspFs,
}

fn start_winfsp_mount(
    mount_point: &U16CStr,
    context: OperonWinFspFs,
) -> Result<WinFspMountHandles, NTSTATUS> {
    let interface = Box::into_raw(Box::new(winfsp_interface()));
    unsafe {
        trace_mount_event(
            "interface_before_create",
            format!(
                "get_volume_info={} get_security_by_name={} create={} create_ex={} open={} overwrite={} read_directory={} get_dir_info_by_name={}",
                (*interface).GetVolumeInfo.is_some(),
                (*interface).GetSecurityByName.is_some(),
                (*interface).Create.is_some(),
                (*interface).CreateEx.is_some(),
                (*interface).Open.is_some(),
                (*interface).Overwrite.is_some(),
                (*interface).ReadDirectory.is_some(),
                (*interface).GetDirInfoByName.is_some(),
            ),
        );
    }
    let volume_params = volume_params();
    let mut file_system = std::ptr::null_mut();
    let device_name = u16cstr!("WinFsp.Disk");

    let status = unsafe {
        FspFileSystemCreate(
            device_name.as_ptr().cast_mut(),
            &volume_params,
            interface,
            &mut file_system,
        )
    };
    trace_mount_event("fs_create", format!("status={status:#x}"));
    if status != STATUS_SUCCESS {
        unsafe {
            drop(Box::from_raw(interface));
        }
        return Err(status);
    }

    let context = Box::into_raw(Box::new(context));
    unsafe {
        (*file_system).UserContext = context.cast();
        let active_interface = (*file_system).Interface;
        if active_interface.is_null() {
            trace_mount_event("interface_after_create", "interface_ptr=NULL");
        } else {
            trace_mount_event(
                "interface_after_create",
                format!(
                    "interface_ptr={active_interface:p} get_volume_info={} get_security_by_name={} create={} create_ex={} open={} overwrite={} read_directory={} get_dir_info_by_name={}",
                    (*active_interface).GetVolumeInfo.is_some(),
                    (*active_interface).GetSecurityByName.is_some(),
                    (*active_interface).Create.is_some(),
                    (*active_interface).CreateEx.is_some(),
                    (*active_interface).Open.is_some(),
                    (*active_interface).Overwrite.is_some(),
                    (*active_interface).ReadDirectory.is_some(),
                    (*active_interface).GetDirInfoByName.is_some(),
                ),
            );
        }
        FspFileSystemSetOperationGuardStrategyF(
            file_system,
            FSP_FILE_SYSTEM_OPERATION_GUARD_STRATEGY_FSP_FILE_SYSTEM_OPERATION_GUARD_STRATEGY_FINE,
        );
    }

    enable_winfsp_debug_log(file_system);

    let status =
        unsafe { FspFileSystemSetMountPoint(file_system, mount_point.as_ptr().cast_mut()) };
    trace_mount_event(
        "set_mount_point",
        format!(
            "status={status:#x} mount_point={}",
            mount_point.to_string_lossy()
        ),
    );
    if status != STATUS_SUCCESS {
        unsafe {
            drop(Box::from_raw(context));
            drop(Box::from_raw(interface));
            FspFileSystemDelete(file_system);
        }
        return Err(status);
    }

    let status = unsafe { FspFileSystemStartDispatcher(file_system, 0) };
    trace_mount_event("start_dispatcher", format!("status={status:#x}"));
    if status != STATUS_SUCCESS {
        unsafe {
            FspFileSystemRemoveMountPoint(file_system);
            drop(Box::from_raw(context));
            drop(Box::from_raw(interface));
            FspFileSystemDelete(file_system);
        }
        return Err(status);
    }

    Ok(WinFspMountHandles {
        file_system,
        interface,
        context,
    })
}

fn enable_winfsp_debug_log(_file_system: *mut FSP_FILE_SYSTEM) {
    #[cfg(feature = "winfsp-debug")]
    unsafe {
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE};

        winfsp_wrs_sys::FspDebugLogSetHandle(GetStdHandle(STD_ERROR_HANDLE));
        winfsp_wrs_sys::FspFileSystemSetDebugLogF(_file_system, u32::MAX);
        trace_mount_event("winfsp_debug", "enabled");
    }
}

fn winfsp_interface() -> FSP_FILE_SYSTEM_INTERFACE {
    FSP_FILE_SYSTEM_INTERFACE {
        GetVolumeInfo: Some(get_volume_info_cb),
        GetSecurityByName: Some(get_security_by_name_cb),
        Create: Some(create_cb),
        CreateEx: Some(create_ex_cb),
        Open: Some(open_cb),
        Overwrite: Some(overwrite_cb),
        Cleanup: Some(cleanup_cb),
        Close: Some(close_cb),
        Read: Some(read_cb),
        Write: Some(write_cb),
        Flush: Some(flush_cb),
        GetFileInfo: Some(get_file_info_cb),
        SetFileSize: Some(set_file_size_cb),
        CanDelete: Some(can_delete_cb),
        Rename: Some(rename_cb),
        GetSecurity: Some(get_security_cb),
        ReadDirectory: Some(read_directory_cb),
        GetDirInfoByName: Some(get_dir_info_by_name_cb),
        DispatcherStopped: Some(dispatcher_stopped_cb),
        ..Default::default()
    }
}

fn volume_params() -> FSP_FSCTL_VOLUME_PARAMS {
    let mut params = FSP_FSCTL_VOLUME_PARAMS::default();
    params.set_UmFileContextIsFullContext(0);
    params.set_UmFileContextIsUserContext2(1);
    params.set_CaseSensitiveSearch(0);
    params.set_CasePreservedNames(1);
    params.set_UnicodeOnDisk(1);
    params.set_PersistentAcls(1);
    params.set_PostCleanupWhenModifiedOnly(1);
    params.set_ReadOnlyVolume(0);
    params.set_AlwaysUseDoubleBuffering(1);
    params.MaxComponentLength = 255;
    params.SectorSize = 4096;
    params.SectorsPerAllocationUnit = 1;
    params.VolumeCreationTime = winfsp_wrs::filetime_now();
    params.VolumeSerialNumber = 0x0A0E_0001;
    params.FileInfoTimeout = 1000;
    params.DirInfoTimeout = 1000;
    params.VolumeInfoTimeout = 1000;
    let name = u16str!("Operon").as_slice();
    params.FileSystemName[..name.len()].copy_from_slice(name);
    params
}

unsafe fn fs_from_raw(file_system: *mut FSP_FILE_SYSTEM) -> &'static OperonWinFspFs {
    &*((*file_system).UserContext.cast::<OperonWinFspFs>())
}

unsafe fn context_from_raw<'a>(file_context: PVOID) -> &'a WindowsFileContext {
    &*(file_context.cast::<WindowsFileContext>())
}

unsafe fn write_context(out: *mut PVOID, stat: &FsStat) {
    let context = Box::new(WindowsFileContext {
        path: stat.path.clone(),
        is_dir: stat.is_dir,
    });
    out.write(Box::into_raw(context).cast());
}

unsafe fn write_file_info(out: *mut FSP_FSCTL_FILE_INFO, stat: &FsStat) {
    out.write(file_info_for_stat(stat));
}

unsafe extern "C" fn get_volume_info_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    volume_info: *mut FSP_FSCTL_VOLUME_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    trace_mount_event("get_volume_info", fs.root_path.clone());
    let label = u16str!("Operon").as_slice();
    let mut info = FSP_FSCTL_VOLUME_INFO {
        TotalSize: 1 << 40,
        FreeSize: 1 << 39,
        VolumeLabelLength: (label.len() * 2) as u16,
        VolumeLabel: [0; 32],
    };
    info.VolumeLabel[..label.len()].copy_from_slice(label);
    volume_info.write(info);
    STATUS_SUCCESS
}

unsafe extern "C" fn get_security_by_name_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_name: PWSTR,
    file_attributes: *mut UINT32,
    security_descriptor: PSECURITY_DESCRIPTOR,
    security_descriptor_size: *mut SIZE_T,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let file_name = U16CStr::from_ptr_str(file_name);
    trace_mount_event("get_security_by_name_enter", file_name.to_string_lossy());
    match fs.stat_for_name(file_name) {
        Ok(stat) => {
            trace_mount_event("get_security_by_name", stat.path.clone());
            if !file_attributes.is_null() {
                file_attributes.write(attributes_for_stat(&stat).0);
            }
            fs.security
                .copy_to(security_descriptor, security_descriptor_size)
        }
        Err(status) => status,
    }
}

fn open_or_create(
    fs: &OperonWinFspFs,
    file_name: &U16CStr,
    create_options: UINT32,
) -> Result<FsStat, NTSTATUS> {
    let path = fs.path_for_name(file_name)?;
    trace_mount_event("create_or_open", path.clone());
    let options = CreateOptions(create_options);
    let stat = match fs.core.stat(&path) {
        Ok(stat) => stat,
        Err(_) => if options.is(CreateOptions::FILE_DIRECTORY_FILE) {
            fs.core.mkdir(&path)
        } else {
            fs.core.create_file(&path)
        }
        .map_err(ntstatus_for_error)?,
    };
    validate_open_options(&stat, options)?;
    Ok(stat)
}

fn validate_open_options(stat: &FsStat, create_options: CreateOptions) -> Result<(), NTSTATUS> {
    if stat.is_dir && create_options.is(CreateOptions::FILE_NON_DIRECTORY_FILE) {
        return Err(STATUS_FILE_IS_A_DIRECTORY);
    }
    if stat.is_file && create_options.is(CreateOptions::FILE_DIRECTORY_FILE) {
        return Err(STATUS_NOT_A_DIRECTORY);
    }
    Ok(())
}

unsafe extern "C" fn create_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_name: PWSTR,
    create_options: UINT32,
    _granted_access: UINT32,
    _file_attributes: UINT32,
    _security_descriptor: PSECURITY_DESCRIPTOR,
    _allocation_size: UINT64,
    file_context: *mut PVOID,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let file_name = U16CStr::from_ptr_str(file_name);
    trace_mount_event("create_enter", file_name.to_string_lossy());
    match open_or_create(fs, file_name, create_options) {
        Ok(stat) => {
            write_context(file_context, &stat);
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn create_ex_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_name: PWSTR,
    create_options: UINT32,
    granted_access: UINT32,
    file_attributes: UINT32,
    security_descriptor: PSECURITY_DESCRIPTOR,
    allocation_size: UINT64,
    _extra_buffer: PVOID,
    _extra_length: ULONG,
    _extra_buffer_is_reparse_point: u8,
    file_context: *mut PVOID,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    let name = U16CStr::from_ptr_str(file_name);
    trace_mount_event("create_ex_enter", name.to_string_lossy());
    create_cb(
        file_system,
        file_name,
        create_options,
        granted_access,
        file_attributes,
        security_descriptor,
        allocation_size,
        file_context,
        file_info,
    )
}

unsafe extern "C" fn open_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_name: PWSTR,
    create_options: UINT32,
    _granted_access: UINT32,
    file_context: *mut PVOID,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let file_name = U16CStr::from_ptr_str(file_name);
    trace_mount_event("open_enter", file_name.to_string_lossy());
    match fs.stat_for_name(file_name).and_then(|stat| {
        validate_open_options(&stat, CreateOptions(create_options))?;
        Ok(stat)
    }) {
        Ok(stat) => {
            trace_mount_event("open", stat.path.clone());
            write_context(file_context, &stat);
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn overwrite_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    _file_attributes: UINT32,
    _replace_file_attributes: u8,
    allocation_size: UINT64,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    if file_context.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    trace_mount_event(
        "overwrite",
        format!("path={} allocation_size={allocation_size}", context.path),
    );
    if context.is_dir {
        return STATUS_FILE_IS_A_DIRECTORY;
    }

    match fs
        .core
        .truncate(&context.path, 0)
        .map_err(ntstatus_for_error)
    {
        Ok(stat) => {
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn cleanup_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    _file_name: PWSTR,
    flags: UINT32,
) {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    if flags & FspCleanupDelete as u32 != 0 {
        let _ = fs.core.delete(&context.path);
    }
}

unsafe extern "C" fn close_cb(_file_system: *mut FSP_FILE_SYSTEM, file_context: PVOID) {
    if !file_context.is_null() {
        drop(Box::from_raw(file_context.cast::<WindowsFileContext>()));
    }
}

unsafe extern "C" fn read_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    buffer: PVOID,
    offset: UINT64,
    length: ULONG,
    bytes_transferred: *mut ULONG,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    trace_mount_event(
        "read",
        format!("path={} offset={offset} length={length}", context.path),
    );
    if context.is_dir {
        return STATUS_FILE_IS_A_DIRECTORY;
    }
    match fs
        .core
        .read_file(&context.path, offset, length)
        .map_err(ntstatus_for_error)
    {
        Ok(data) => {
            let copied = data.len().min(length as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr(), buffer.cast::<u8>(), copied);
            bytes_transferred.write(copied as ULONG);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn write_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    buffer: PVOID,
    offset: UINT64,
    length: ULONG,
    write_to_eof: u8,
    _constrained_io: u8,
    bytes_transferred: *mut ULONG,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    if context.is_dir {
        return STATUS_FILE_IS_A_DIRECTORY;
    }
    let offset = if write_to_eof != 0 {
        match fs.core.stat(&context.path) {
            Ok(stat) => stat.size,
            Err(error) => return ntstatus_for_error(error),
        }
    } else {
        offset
    };
    let data = std::slice::from_raw_parts(buffer.cast::<u8>(), length as usize);
    match fs
        .core
        .write_file(&context.path, offset, data)
        .and_then(|written| fs.core.stat(&context.path).map(|stat| (written, stat)))
        .map_err(ntstatus_for_error)
    {
        Ok((written, stat)) => {
            bytes_transferred.write(written.min(u32::MAX as u64) as ULONG);
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn flush_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    if file_context.is_null() {
        return STATUS_SUCCESS;
    }
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    match fs.core.stat(&context.path).map_err(ntstatus_for_error) {
        Ok(stat) => {
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn get_file_info_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    flush_cb(file_system, file_context, file_info)
}

unsafe extern "C" fn set_file_size_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    new_size: UINT64,
    _set_allocation_size: u8,
    file_info: *mut FSP_FSCTL_FILE_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    if context.is_dir {
        return STATUS_FILE_IS_A_DIRECTORY;
    }
    match fs
        .core
        .truncate(&context.path, new_size)
        .map_err(ntstatus_for_error)
    {
        Ok(stat) => {
            write_file_info(file_info, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn can_delete_cb(
    _file_system: *mut FSP_FILE_SYSTEM,
    _file_context: PVOID,
    _file_name: PWSTR,
) -> NTSTATUS {
    STATUS_SUCCESS
}

unsafe extern "C" fn rename_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    _file_name: PWSTR,
    new_file_name: PWSTR,
    _replace_if_exists: u8,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    let new_file_name = U16CStr::from_ptr_str(new_file_name);
    let new_path = match fs.path_for_name(new_file_name) {
        Ok(path) => path,
        Err(status) => return status,
    };
    fs.core
        .rename(&context.path, &new_path)
        .map(|()| STATUS_SUCCESS)
        .unwrap_or_else(ntstatus_for_error)
}

unsafe extern "C" fn get_security_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    _file_context: PVOID,
    security_descriptor: PSECURITY_DESCRIPTOR,
    security_descriptor_size: *mut SIZE_T,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    fs.security
        .copy_to(security_descriptor, security_descriptor_size)
}

unsafe extern "C" fn read_directory_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    _pattern: PWSTR,
    marker: PWSTR,
    buffer: PVOID,
    length: ULONG,
    bytes_transferred: *mut ULONG,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    if !context.is_dir {
        return STATUS_NOT_A_DIRECTORY;
    }
    let marker = if marker.is_null() {
        None
    } else {
        Some(U16CStr::from_ptr_str(marker).to_string_lossy())
    };
    trace_mount_event(
        "read_directory",
        format!("path={} marker={marker:?}", context.path),
    );
    let entries = match fs.core.list_dir(&context.path).map_err(ntstatus_for_error) {
        Ok(entries) => entries,
        Err(status) => return status,
    };
    let mut seen_marker = marker.is_none();
    for entry in entries {
        if !seen_marker {
            seen_marker = marker.as_deref() == Some(entry.name.as_str());
            continue;
        }
        if !add_dir_info(buffer, length, bytes_transferred, &entry.name, &entry.stat) {
            break;
        }
    }
    STATUS_SUCCESS
}

unsafe extern "C" fn get_dir_info_by_name_cb(
    file_system: *mut FSP_FILE_SYSTEM,
    file_context: PVOID,
    file_name: PWSTR,
    dir_info: *mut FSP_FSCTL_DIR_INFO,
) -> NTSTATUS {
    let fs = fs_from_raw(file_system);
    let context = context_from_raw(file_context);
    if !context.is_dir {
        return STATUS_NOT_A_DIRECTORY;
    }
    let child = U16CStr::from_ptr_str(file_name).to_string_lossy();
    trace_mount_event(
        "get_dir_info_by_name",
        format!("parent={} child={child}", context.path),
    );
    let path = match join_remote_child(&context.path, &child) {
        Ok(path) => path,
        Err(_) => return STATUS_INVALID_PARAMETER,
    };
    match fs.core.stat(&path).map_err(ntstatus_for_error) {
        Ok(stat) => {
            write_dir_info(dir_info, &child, &stat);
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn dispatcher_stopped_cb(_file_system: *mut FSP_FILE_SYSTEM, normally: u8) {
    trace_mount_event("dispatcher_stopped", format!("normally={}", normally != 0));
}

fn add_dir_info(
    buffer: PVOID,
    length: ULONG,
    bytes_transferred: *mut ULONG,
    name: &str,
    stat: &FsStat,
) -> bool {
    let mut storage = dir_info_storage(name, stat);
    unsafe {
        FspFileSystemAddDirInfo(
            storage.as_mut_ptr().cast::<FSP_FSCTL_DIR_INFO>(),
            buffer,
            length,
            bytes_transferred,
        ) != 0
    }
}

fn write_dir_info(out: *mut FSP_FSCTL_DIR_INFO, name: &str, stat: &FsStat) {
    let mut storage = dir_info_storage(name, stat);
    unsafe {
        std::ptr::copy_nonoverlapping(storage.as_mut_ptr(), out.cast::<u8>(), storage.len());
    }
}

fn dir_info_storage(name: &str, stat: &FsStat) -> Vec<u8> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let size = std::mem::size_of::<FSP_FSCTL_DIR_INFO>() + name.len() * 2;
    let mut storage = vec![0u8; size];
    unsafe {
        let dir_info = storage.as_mut_ptr().cast::<FSP_FSCTL_DIR_INFO>();
        (*dir_info).Size = size as u16;
        (*dir_info).FileInfo = file_info_for_stat(stat);
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (*dir_info).FileNameBuf.as_mut_ptr(),
            name.len(),
        );
    }
    storage
}
