#![cfg(windows)]

use anyhow::Context;
use winfsp_wrs::U16CStr;

use crate::mount_core::{join_remote_child, normalize_remote_path};

pub(super) fn windows_name_to_remote_path(root: &str, name: &U16CStr) -> anyhow::Result<String> {
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
        path = join_remote_child(&path, component)
            .with_context(|| format!("invalid Windows mount path component `{component}`"))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winfsp_wrs::u16cstr;

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
