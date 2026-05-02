use std::ffi::OsStr;

pub(crate) fn normalize_remote_path(path: &str) -> anyhow::Result<String> {
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

pub(crate) fn validate_child_name(name: &OsStr) -> anyhow::Result<&str> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path names must be valid UTF-8"))?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.as_bytes().contains(&0)
    {
        anyhow::bail!("invalid child path name");
    }
    Ok(name)
}

pub(crate) fn join_remote_child(parent: &str, child: &str) -> anyhow::Result<String> {
    let parent = normalize_remote_path(parent)?;
    if child.is_empty() || child == "." || child == ".." || child.contains('/') {
        anyhow::bail!("invalid child path name");
    }
    if parent == "/" {
        Ok(format!("/{child}"))
    } else {
        Ok(format!("{parent}/{child}"))
    }
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
}
