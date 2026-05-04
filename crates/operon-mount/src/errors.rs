#![cfg(any(target_os = "linux", target_os = "macos"))]

use crate::mount_core::{classify_mount_error, MountErrorKind};

pub(crate) fn errno_for_error(error: &anyhow::Error) -> fuser::Errno {
    match classify_mount_error(error) {
        MountErrorKind::NotFound => fuser::Errno::ENOENT,
        MountErrorKind::PermissionDenied => fuser::Errno::EACCES,
        MountErrorKind::InvalidInput => fuser::Errno::EINVAL,
        MountErrorKind::FailedPrecondition => fuser::Errno::EPERM,
        MountErrorKind::Unknown => fuser::Errno::EIO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tonic_statuses_to_fuse_errno() {
        assert_eq!(
            errno_debug(errno_for_error(&tonic::Status::not_found("missing").into())),
            errno_debug(fuser::Errno::ENOENT)
        );
        assert_eq!(
            errno_debug(errno_for_error(
                &tonic::Status::permission_denied("denied").into()
            )),
            errno_debug(fuser::Errno::EACCES)
        );
        assert_eq!(
            errno_debug(errno_for_error(
                &tonic::Status::unauthenticated("missing token").into()
            )),
            errno_debug(fuser::Errno::EACCES)
        );
        assert_eq!(
            errno_debug(errno_for_error(
                &tonic::Status::invalid_argument("bad path").into()
            )),
            errno_debug(fuser::Errno::EINVAL)
        );
        assert_eq!(
            errno_debug(errno_for_error(
                &tonic::Status::failed_precondition("version").into()
            )),
            errno_debug(fuser::Errno::EPERM)
        );
    }

    #[test]
    fn maps_unknown_errors_to_io_error() {
        assert_eq!(
            errno_debug(errno_for_error(&anyhow::anyhow!("plain error"))),
            errno_debug(fuser::Errno::EIO)
        );
    }

    fn errno_debug(errno: fuser::Errno) -> String {
        format!("{errno:?}")
    }
}
