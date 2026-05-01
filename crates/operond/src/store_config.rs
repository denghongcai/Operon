use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use operon_config::resolve_path;

pub(crate) fn resolve_store_path(
    config_dir: &Path,
    configured: Option<&Path>,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(configured) = configured else {
        return Ok(None);
    };
    let path = resolve_path(config_dir, configured);
    validate_store_path(config_dir, &path)?;
    Ok(Some(path))
}

fn validate_store_path(config_dir: &Path, path: &Path) -> anyhow::Result<()> {
    let config_root = config_dir
        .canonicalize()
        .with_context(|| format!("failed to canonicalize config dir {}", config_dir.display()))?;
    let mut ancestor = path.parent().unwrap_or(config_dir);
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("store path has no existing parent"))?;
    }
    let ancestor = ancestor
        .canonicalize()
        .with_context(|| format!("failed to canonicalize store parent {}", ancestor.display()))?;
    if !ancestor.starts_with(&config_root) {
        anyhow::bail!(
            "daemon store path {} must stay under config directory {}",
            path.display(),
            config_root.display()
        );
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            anyhow::bail!("daemon store path {} must not be a symlink", path.display());
        }
        if !file_type.is_file() {
            anyhow::bail!(
                "daemon store path {} must be a regular file",
                path.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_path_must_stay_under_config_dir() {
        let base = unique_temp_dir("operond-store-path-test");
        let config_dir = base.join("config");
        let outside = base.join("outside");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&outside).expect("outside dir");

        let allowed = resolve_store_path(&config_dir, Some(Path::new("store.jsonl")))
            .expect("relative store path");
        assert_eq!(allowed, Some(config_dir.join("store.jsonl")));

        let denied = resolve_store_path(&config_dir, Some(&outside.join("store.jsonl")))
            .expect_err("absolute outside store path should be denied");
        assert!(denied
            .to_string()
            .contains("must stay under config directory"));
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn store_path_rejects_symlink() {
        let base = unique_temp_dir("operond-store-symlink-test");
        let config_dir = base.join("config");
        fs::create_dir_all(&config_dir).expect("config dir");
        let target = config_dir.join("target.jsonl");
        let link = config_dir.join("store.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let denied = resolve_store_path(&config_dir, Some(Path::new("store.jsonl")))
            .expect_err("symlink store path should be denied");
        assert!(denied.to_string().contains("must not be a symlink"));
        let _ = fs::remove_dir_all(base);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }
}
