use std::ffi::OsStr;

use operon_core::{FsList, FsStat};

/// Platform-neutral filesystem operations needed by live mount adapters.
pub trait RemoteFs: Send + Sync {
    fn stat(&self, path: &str) -> anyhow::Result<FsStat>;
    fn list(&self, path: &str) -> anyhow::Result<FsList>;
    fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>>;
    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64>;
    fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat>;
    fn mkdir(&self, path: &str) -> anyhow::Result<FsStat>;
    fn delete(&self, path: &str) -> anyhow::Result<()>;
    fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()>;
}

pub fn normalize_remote_path(path: &str) -> anyhow::Result<String> {
    if !path.starts_with('/') {
        anyhow::bail!("remote mount path must be absolute");
    }
    if path.as_bytes().contains(&0) {
        anyhow::bail!("remote mount path cannot contain NUL");
    }

    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => anyhow::bail!("remote mount path cannot contain `..`"),
            value => parts.push(value),
        }
    }

    if parts.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", parts.join("/")))
    }
}

pub fn validate_child_name(name: &OsStr) -> anyhow::Result<&str> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path names must be valid UTF-8"))?;
    validate_child_path_segment(name)?;
    Ok(name)
}

pub fn join_remote_child(parent: &str, child: &str) -> anyhow::Result<String> {
    let parent = normalize_remote_path(parent)?;
    validate_child_path_segment(child)?;
    if parent == "/" {
        Ok(format!("/{child}"))
    } else {
        Ok(format!("{parent}/{child}"))
    }
}

fn validate_child_path_segment(child: &str) -> anyhow::Result<()> {
    if child.is_empty()
        || child == "."
        || child == ".."
        || child.contains('/')
        || child.as_bytes().contains(&0)
    {
        anyhow::bail!("invalid child path name");
    }
    Ok(())
}

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

    #[test]
    fn joins_remote_child_under_normalized_parent() {
        assert_eq!(
            join_remote_child("/workspace//project/", "file.txt").expect("child path"),
            "/workspace/project/file.txt"
        );
    }

    #[test]
    fn rejects_invalid_child_segments() {
        for child in ["", ".", "..", "nested/file", "nul\0byte"] {
            let error = join_remote_child("/workspace", child).expect_err("invalid child");
            assert!(error.to_string().contains("invalid child path name"));
        }
    }
}
