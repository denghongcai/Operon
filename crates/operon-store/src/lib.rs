use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use operon_core::JobRecord;

pub const DEFAULT_STORE_PATH: &str = "operon.db";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FsyncPolicy {
    #[default]
    Always,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct StoreWriter {
    path: Option<PathBuf>,
    fsync_policy: FsyncPolicy,
}

impl StoreWriter {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            fsync_policy: FsyncPolicy::default(),
        }
    }

    pub fn with_fsync_policy(mut self, fsync_policy: FsyncPolicy) -> Self {
        self.fsync_policy = fsync_policy;
        self
    }

    pub fn append_json_value(&self, record: &serde_json::Value) -> anyhow::Result<()> {
        append_record_with_policy(self.path.as_deref(), record, self.fsync_policy)
    }
}

pub fn append_record(path: Option<&Path>, record: &serde_json::Value) -> anyhow::Result<()> {
    append_record_with_policy(path, record, FsyncPolicy::default())
}

fn append_record_with_policy(
    path: Option<&Path>,
    record: &serde_json::Value,
    fsync_policy: FsyncPolicy,
) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create store directory {}", parent.display()))?;
    }
    let line = serde_json::to_string(record).context("failed to serialize store record")?;
    open_store_file(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{line}")?;
            if fsync_policy == FsyncPolicy::Always {
                file.sync_data()?;
            }
            Ok(())
        })
        .with_context(|| format!("failed to append store record {}", path.display()))
}

fn open_store_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "store path is not a regular file",
        ));
    }
    Ok(file)
}

pub fn load_jobs(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, JobRecord>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let mut jobs = BTreeMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)?;
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("job") {
            continue;
        }
        if let Some(record) = value.get("record") {
            let record: JobRecord = serde_json::from_value(record.clone())?;
            jobs.insert(record.id.clone(), record);
        }
    }
    Ok(jobs)
}

pub fn default_store_path() -> PathBuf {
    PathBuf::from(DEFAULT_STORE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_path_is_stable() {
        assert_eq!(DEFAULT_STORE_PATH, "operon.db");
    }

    #[cfg(unix)]
    #[test]
    fn append_record_rejects_symlink_store_path() {
        let base = unique_temp_dir("operon-store-symlink-test");
        std::fs::create_dir_all(&base).expect("base dir");
        let target = base.join("target.jsonl");
        let link = base.join("link.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let error = append_record(Some(&link), &serde_json::json!({"kind": "audit"}))
            .expect_err("symlink store path should be rejected");

        assert!(error.to_string().contains("failed to append store record"));
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn append_record_writes_json_line() {
        let base = unique_temp_dir("operon-store-append-test");
        std::fs::create_dir_all(&base).expect("base dir");
        let store = base.join("store.jsonl");

        append_record(Some(&store), &serde_json::json!({"kind": "audit"})).expect("append record");

        let content = std::fs::read_to_string(&store).expect("store content");
        assert_eq!(content, "{\"kind\":\"audit\"}\n");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn store_writer_can_disable_fsync_for_tests() {
        let base = unique_temp_dir("operon-store-writer-test");
        std::fs::create_dir_all(&base).expect("base dir");
        let store = base.join("store.jsonl");
        let writer = StoreWriter::new(Some(store.clone())).with_fsync_policy(FsyncPolicy::Disabled);

        writer
            .append_json_value(&serde_json::json!({"kind": "job"}))
            .expect("append record");

        let content = std::fs::read_to_string(&store).expect("store content");
        assert_eq!(content, "{\"kind\":\"job\"}\n");
        let _ = std::fs::remove_dir_all(base);
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
