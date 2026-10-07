use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use operon_core::{AuditEvent, ExecLog, ExecRecord};

pub const DEFAULT_STORE_PATH: &str = "operon.db";

mod recovery;
mod writer;
pub use recovery::recover_store;

// Includes independent writers and the legacy append_record entrypoint.
// A complete record may require more than one OS write, even with O_APPEND.
static APPEND_LOCK: Mutex<()> = Mutex::new(());

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
    worker: Arc<Mutex<Option<Arc<writer::Worker>>>>,
}

impl StoreWriter {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            fsync_policy: FsyncPolicy::default(),
            worker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_fsync_policy(mut self, fsync_policy: FsyncPolicy) -> Self {
        self.fsync_policy = fsync_policy;
        self
    }

    pub fn append_json_value(&self, record: &serde_json::Value) -> anyhow::Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        let mut line = serde_json::to_vec(record).context("failed to serialize store record")?;
        line.push(b'\n');
        self.worker(path)?
            .append(line, self.fsync_policy)
            .with_context(|| format!("failed to append store record {}", path.display()))
    }

    fn worker(&self, path: &Path) -> anyhow::Result<Arc<writer::Worker>> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| anyhow::anyhow!("store writer lock poisoned"))?;
        if worker.is_none() {
            *worker = Some(writer::worker_for(path)?);
        }
        worker
            .clone()
            .ok_or_else(|| anyhow::anyhow!("store writer unavailable"))
    }

    /// Ordered barrier: all preceding records have been written and synced.
    pub fn flush(&self) -> anyhow::Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        self.worker(path)?.append(Vec::new(), FsyncPolicy::Always)
    }
}

pub fn append_record(path: Option<&Path>, record: &serde_json::Value) -> anyhow::Result<()> {
    StoreWriter::new(path.map(Path::to_path_buf)).append_json_value(record)
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

pub fn load_execs(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, ExecRecord>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let mut execs = BTreeMap::new();
    for value in recovery::load_records(path)? {
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("exec") {
            continue;
        }
        if let Some(record) = value.get("record") {
            let record: ExecRecord = serde_json::from_value(record.clone())?;
            execs.insert(record.id.clone(), record);
        }
    }
    Ok(execs)
}

pub fn load_audit_events(path: Option<&Path>) -> anyhow::Result<Vec<AuditEvent>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut events = Vec::new();
    for value in recovery::load_records(path)? {
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("audit") {
            continue;
        }
        if let Some(event) = value.get("event") {
            events.push(serde_json::from_value(event.clone())?);
        }
    }
    Ok(events)
}

pub fn load_exec_logs(path: Option<&Path>) -> anyhow::Result<BTreeMap<String, Vec<ExecLog>>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let mut logs = BTreeMap::<String, Vec<ExecLog>>::new();
    for value in recovery::load_records(path)? {
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("exec_log") {
            continue;
        }
        let Some(exec_id) = value.get("exec_id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if let Some(log) = value.get("log") {
            logs.entry(exec_id.to_string())
                .or_default()
                .push(serde_json::from_value(log.clone())?);
        }
    }
    Ok(logs)
}

pub fn default_store_path() -> PathBuf {
    PathBuf::from(DEFAULT_STORE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_writers_preserve_complete_records_and_producer_order() {
        for policy in [FsyncPolicy::Always, FsyncPolicy::Disabled] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("store");
            let writer = StoreWriter::new(Some(path.clone())).with_fsync_policy(policy);
            std::thread::scope(|scope| {
                for producer in 0..8 {
                    // Exercise both clones and independently constructed writers.
                    let writer = if producer % 2 == 0 {
                        writer.clone()
                    } else {
                        StoreWriter::new(Some(path.clone())).with_fsync_policy(policy)
                    };
                    scope.spawn(move || {
                        for sequence in 0..30 {
                            writer
                                .append_json_value(&serde_json::json!({
                                    "kind": (["audit", "exec", "exec_log"][sequence % 3]),
                                    "producer": producer, "sequence": sequence,
                                    "text": r#"中文 \"{}\""#,
                                }))
                                .unwrap();
                        }
                    });
                }
            });
            let content = std::fs::read_to_string(path).unwrap();
            let mut next = [0; 8];
            for line in content.lines() {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                let producer = value["producer"].as_u64().unwrap() as usize;
                assert_eq!(value["sequence"].as_u64().unwrap(), next[producer]);
                next[producer] += 1;
            }
            assert_eq!(next, [30; 8]);
        }
    }

    #[test]
    fn default_store_path_is_stable() {
        assert_eq!(DEFAULT_STORE_PATH, "operon.db");
    }

    #[cfg(unix)]
    #[test]
    fn append_record_rejects_symlink_store_path() {
        let base = tempfile::tempdir().expect("temp dir");
        let target = base.path().join("target.jsonl");
        let link = base.path().join("link.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let error = append_record(Some(&link), &serde_json::json!({"kind": "audit"}))
            .expect_err("symlink store path should be rejected");

        assert!(error.to_string().contains("failed to append store record"));
        assert!(!target.exists());
    }

    #[test]
    fn append_record_writes_json_line() {
        let base = tempfile::tempdir().expect("temp dir");
        let store = base.path().join("store.jsonl");

        append_record(Some(&store), &serde_json::json!({"kind": "audit"})).expect("append record");

        let content = std::fs::read_to_string(&store).expect("store content");
        assert_eq!(content, "{\"kind\":\"audit\"}\n");
    }

    #[test]
    fn store_writer_can_disable_fsync_for_tests() {
        let base = tempfile::tempdir().expect("temp dir");
        let store = base.path().join("store.jsonl");
        let writer = StoreWriter::new(Some(store.clone())).with_fsync_policy(FsyncPolicy::Disabled);

        writer
            .append_json_value(&serde_json::json!({"kind": "exec"}))
            .expect("append record");

        let content = std::fs::read_to_string(&store).expect("store content");
        assert_eq!(content, "{\"kind\":\"exec\"}\n");
    }

    #[test]
    fn load_audit_events_reads_persisted_audit_records_in_order() {
        let base = tempfile::tempdir().expect("temp dir");
        let store = base.path().join("store.jsonl");
        let first = test_audit_event("stat", 100);
        let second = test_audit_event("read", 200);

        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "exec",
                "record": {"id": "exec-1"}
            }),
        )
        .expect("append non-audit");
        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "audit",
                "event": first,
            }),
        )
        .expect("append first audit");
        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "audit",
                "event": second,
            }),
        )
        .expect("append second audit");

        let events = load_audit_events(Some(&store)).expect("load audit events");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, "stat");
        assert_eq!(events[1].action, "read");
    }

    #[test]
    fn load_exec_logs_reads_persisted_log_records_by_exec() {
        let base = tempfile::tempdir().expect("temp dir");
        let store = base.path().join("store.jsonl");
        let first = test_exec_log("stdout", b"hello".to_vec(), 0);
        let second = test_exec_log("stderr", b"warn".to_vec(), 1);

        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "audit",
                "event": test_audit_event("stat", 100),
            }),
        )
        .expect("append non-log");
        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "exec_log",
                "exec_id": "exec-1",
                "log": first,
                "dropped_log_count": 0,
            }),
        )
        .expect("append first log");
        append_record(
            Some(&store),
            &serde_json::json!({
                "kind": "exec_log",
                "exec_id": "exec-1",
                "log": second,
                "dropped_log_count": 0,
            }),
        )
        .expect("append second log");

        let logs = load_exec_logs(Some(&store)).expect("load exec logs");
        let exec_logs = logs.get("exec-1").expect("exec logs");

        assert_eq!(exec_logs.len(), 2);
        assert_eq!(exec_logs[0].stream, "stdout");
        assert_eq!(exec_logs[0].data, b"hello");
        assert_eq!(exec_logs[1].sequence, 1);
    }

    fn test_audit_event(action: &str, timestamp_ms: u64) -> AuditEvent {
        AuditEvent {
            subject: "test-subject".to_string(),
            timestamp_ms,
            node_id: "node-a".to_string(),
            capability: "fs:workspace".to_string(),
            action: action.to_string(),
            resource: "/".to_string(),
            allowed: true,
            reason: "allowed".to_string(),
            run_id: None,
            step_id: None,
        }
    }

    fn test_exec_log(stream: &str, data: Vec<u8>, sequence: u64) -> ExecLog {
        ExecLog {
            stream: stream.to_string(),
            data,
            sequence,
        }
    }
}
