#![cfg(windows)]

use windows_sys::Win32::Foundation::STATUS_OBJECT_NAME_COLLISION;
use winfsp_wrs::{
    NTSTATUS, STATUS_ACCESS_DENIED, STATUS_INVALID_PARAMETER, STATUS_NOT_IMPLEMENTED,
    STATUS_OBJECT_NAME_NOT_FOUND,
};

use crate::mount_core::{classify_mount_error, MountErrorKind};

pub(super) fn ntstatus_for_error(error: anyhow::Error) -> NTSTATUS {
    match classify_mount_error(&error) {
        MountErrorKind::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
        MountErrorKind::AlreadyExists => STATUS_OBJECT_NAME_COLLISION,
        MountErrorKind::PermissionDenied => STATUS_ACCESS_DENIED,
        MountErrorKind::InvalidInput => STATUS_INVALID_PARAMETER,
        MountErrorKind::FailedPrecondition => STATUS_ACCESS_DENIED,
        MountErrorKind::Unknown => STATUS_NOT_IMPLEMENTED,
    }
}
