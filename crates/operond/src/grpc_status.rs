use operon_core::RuntimeErrorKind;
use tonic::Status;

pub(crate) fn status_from_error(error: (RuntimeErrorKind, String)) -> Status {
    match error.0 {
        RuntimeErrorKind::Forbidden => Status::permission_denied(error.1),
        RuntimeErrorKind::NotFound => Status::not_found(error.1),
        RuntimeErrorKind::AlreadyExists => Status::already_exists(error.1),
        RuntimeErrorKind::InvalidArgument => Status::invalid_argument(error.1),
        RuntimeErrorKind::Internal => Status::internal(error.1),
    }
}

pub(crate) fn status_from_io_error(error: std::io::Error) -> Status {
    match error.kind() {
        std::io::ErrorKind::NotFound => Status::not_found(error.to_string()),
        std::io::ErrorKind::PermissionDenied => Status::permission_denied(error.to_string()),
        std::io::ErrorKind::AlreadyExists => Status::already_exists(error.to_string()),
        std::io::ErrorKind::InvalidInput => Status::invalid_argument(error.to_string()),
        std::io::ErrorKind::DirectoryNotEmpty => {
            let mut status = Status::failed_precondition(error.to_string());
            status.metadata_mut().insert(
                operon_protocol::FS_ERROR_KIND_METADATA,
                tonic::metadata::MetadataValue::from_static(
                    operon_protocol::FS_ERROR_DIRECTORY_NOT_EMPTY,
                ),
            );
            status
        }
        _ => Status::internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty_directory_has_portable_typed_status() {
        let status =
            status_from_io_error(std::io::Error::from(std::io::ErrorKind::DirectoryNotEmpty));
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            status
                .metadata()
                .get(operon_protocol::FS_ERROR_KIND_METADATA)
                .unwrap()
                .to_str()
                .unwrap(),
            operon_protocol::FS_ERROR_DIRECTORY_NOT_EMPTY,
        );
    }
}
