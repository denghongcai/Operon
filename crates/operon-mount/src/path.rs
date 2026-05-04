pub(crate) use crate::mount_core::{join_remote_child, normalize_remote_path, validate_child_name};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_remote_root_paths() {
        assert_eq!(normalize_remote_path("/").expect("root"), "/");
        assert_eq!(
            normalize_remote_path("/workspace//project/").expect("path"),
            "/workspace/project"
        );
    }

    #[test]
    fn rejects_mount_root_escape_paths() {
        let error = normalize_remote_path("/workspace/../secret").expect_err("escape");

        assert!(error.to_string().contains("cannot contain `..`"));
    }
}
