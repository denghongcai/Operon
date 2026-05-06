#![cfg(any(target_os = "linux", target_os = "macos"))]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XattrRequest {
    Get,
    List { size: u32 },
    Set,
    Remove,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum XattrDecision {
    Error(fuser::Errno),
    EmptyListSize,
    EmptyListData,
}

pub(crate) fn rename_flags_errno(flags: fuser::RenameFlags) -> Option<fuser::Errno> {
    (!flags.is_empty()).then_some(fuser::Errno::ENOSYS)
}

pub(crate) fn xattr_decision(
    access_errno: Option<fuser::Errno>,
    request: XattrRequest,
) -> XattrDecision {
    if let Some(errno) = access_errno {
        return XattrDecision::Error(errno);
    }

    match request {
        XattrRequest::Get | XattrRequest::Remove => XattrDecision::Error(fuser::Errno::NO_XATTR),
        XattrRequest::List { size: 0 } => XattrDecision::EmptyListSize,
        XattrRequest::List { .. } => XattrDecision::EmptyListData,
        XattrRequest::Set => XattrDecision::Error(fuser::Errno::ENOTSUP),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_flags_are_rejected_explicitly() {
        assert!(rename_flags_errno(fuser::RenameFlags::empty()).is_none());
        assert_eq!(
            errno_name(rename_flags_errno(fuser::RenameFlags::from_bits_retain(1))),
            Some(format!("{:?}", fuser::Errno::ENOSYS))
        );
    }

    #[test]
    fn xattr_semantics_preserve_missing_inode_and_empty_list_behavior() {
        assert_eq!(
            decision_name(xattr_decision(
                Some(fuser::Errno::ENOENT),
                XattrRequest::Get
            )),
            format!("error:{:?}", fuser::Errno::ENOENT)
        );
        assert_eq!(
            decision_name(xattr_decision(None, XattrRequest::Get)),
            format!("error:{:?}", fuser::Errno::NO_XATTR)
        );
        assert_eq!(
            decision_name(xattr_decision(None, XattrRequest::List { size: 0 })),
            "empty-list-size"
        );
        assert_eq!(
            decision_name(xattr_decision(None, XattrRequest::List { size: 128 })),
            "empty-list-data"
        );
        assert_eq!(
            decision_name(xattr_decision(None, XattrRequest::Set)),
            format!("error:{:?}", fuser::Errno::ENOTSUP)
        );
        assert_eq!(
            decision_name(xattr_decision(None, XattrRequest::Remove)),
            format!("error:{:?}", fuser::Errno::NO_XATTR)
        );
    }

    fn errno_name(errno: Option<fuser::Errno>) -> Option<String> {
        errno.map(|errno| format!("{errno:?}"))
    }

    fn decision_name(decision: XattrDecision) -> String {
        match decision {
            XattrDecision::Error(errno) => format!("error:{errno:?}"),
            XattrDecision::EmptyListSize => "empty-list-size".to_string(),
            XattrDecision::EmptyListData => "empty-list-data".to_string(),
        }
    }
}
