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
    let mut options = std::fs::OpenOptions::new();
    if let Err(error) = options
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{line}")
        })
    {
        tracing_warn(&format!(
            "failed to append store record {}: {error}",
            path.display()
        ));
    }
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
}
