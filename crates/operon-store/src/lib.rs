use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use operon_core::JobRecord;

pub const DEFAULT_STORE_PATH: &str = "operon.db";

pub fn append_record(path: Option<&Path>, record: &serde_json::Value) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing_warn(&format!(
                "failed to create store directory {}: {error}",
                parent.display()
            ));
            return;
        }
    }
    let line = match serde_json::to_string(record) {
        Ok(line) => line,
        Err(error) => {
            tracing_warn(&format!("failed to serialize store record: {error}"));
            return;
        }
    };
    if let Err(error) = open_store_file(path).and_then(|mut file| {
        use std::io::Write;
        writeln!(file, "{line}")?;
        file.sync_data()
    }) {
        tracing_warn(&format!(
            "failed to append store record {}: {error}",
            path.display()
        ));
    }
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

fn tracing_warn(message: &str) {
    eprintln!("{message}");
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

        append_record(Some(&link), &serde_json::json!({"kind": "audit"}));

        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn append_record_writes_json_line() {
        let base = unique_temp_dir("operon-store-append-test");
        std::fs::create_dir_all(&base).expect("base dir");
        let store = base.join("store.jsonl");

        append_record(Some(&store), &serde_json::json!({"kind": "audit"}));

        let content = std::fs::read_to_string(&store).expect("store content");
        assert_eq!(content, "{\"kind\":\"audit\"}\n");
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
