pub(crate) fn errno_for_error(error: &anyhow::Error) -> fuser::Errno {
    if let Some(status) = error.downcast_ref::<tonic::Status>() {
        match status.code() {
            tonic::Code::NotFound => return fuser::Errno::ENOENT,
            tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => {
                return fuser::Errno::EACCES;
            }
            tonic::Code::InvalidArgument => return fuser::Errno::EINVAL,
            tonic::Code::FailedPrecondition => return fuser::Errno::EPERM,
            _ => {}
        }
    }

    fuser::Errno::EIO
}
