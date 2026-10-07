use super::FsyncPolicy;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex, OnceLock, Weak,
    },
    thread::JoinHandle,
};

const QUEUE_CAPACITY: usize = 256;
const MAX_BATCH_RECORDS: usize = 64;
type Reply = SyncSender<Result<(), String>>;
struct Append {
    data: Vec<u8>,
    policy: FsyncPolicy,
    reply: Reply,
}

#[derive(Debug)]
pub(crate) struct Worker {
    sender: Option<SyncSender<Append>>,
    thread: Option<JoinHandle<()>>,
}

static WRITERS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Worker>>>> = OnceLock::new();

pub(crate) fn worker_for(path: &Path) -> anyhow::Result<Arc<Worker>> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path = match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => absolute,
    };
    let mut registry = WRITERS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("store writer registry poisoned"))?;
    registry.retain(|_, worker| worker.strong_count() > 0);
    if let Some(worker) = registry.get(&path).and_then(Weak::upgrade) {
        return Ok(worker);
    }
    let (sender, receiver) = mpsc::sync_channel::<Append>(QUEUE_CAPACITY);
    let worker_path = path.clone();
    let thread = std::thread::Builder::new()
        .name("operon-store".into())
        .spawn(move || {
            let opened = (|| -> anyhow::Result<std::fs::File> {
                let _guard = super::APPEND_LOCK
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store append lock poisoned"))?;
                if let Some(parent) = worker_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                super::recovery::recover_store_unlocked(&worker_path)?;
                Ok(super::open_store_file(&worker_path)?)
            })();
            run_writer(receiver, opened.map_err(|error| format!("{error:#}")));
        })?;
    let worker = Arc::new(Worker {
        sender: Some(sender),
        thread: Some(thread),
    });
    registry.insert(path, Arc::downgrade(&worker));
    Ok(worker)
}

fn run_writer<S: Sink>(receiver: mpsc::Receiver<Append>, opened: Result<S, String>) -> Option<S> {
    let mut failure = opened.as_ref().err().cloned();
    let mut file = opened.ok();
    while let Ok(first) = receiver.recv() {
        let mut batch = vec![first];
        while batch.len() < MAX_BATCH_RECORDS {
            match receiver.try_recv() {
                Ok(record) => batch.push(record),
                Err(_) => break,
            }
        }
        if failure.is_none() {
            let result = match file.as_mut() {
                Some(file) => write_batch(file, &batch),
                None => Err(std::io::Error::other("store file unavailable")),
            };
            if let Err(error) = result {
                failure = Some(error.to_string());
            }
        }
        // Sticky failure: never append after a possibly partial write.
        // Acknowledgements follow the entire group's successful sync.
        for record in batch {
            let _ = record.reply.send(failure.clone().map_or(Ok(()), Err));
        }
    }
    file
}

fn write_batch(file: &mut impl Sink, batch: &[Append]) -> std::io::Result<()> {
    let _guard = super::APPEND_LOCK
        .lock()
        .map_err(|_| std::io::Error::other("store append lock poisoned"))?;
    for record in batch {
        file.write_all(&record.data)?;
    }
    if batch
        .iter()
        .any(|record| record.policy == FsyncPolicy::Always)
    {
        file.sync()?;
    }
    Ok(())
}

trait Sink: Write {
    fn sync(&mut self) -> std::io::Result<()>;
}
impl Sink for std::fs::File {
    fn sync(&mut self) -> std::io::Result<()> {
        self.sync_data()
    }
}

impl Worker {
    pub(crate) fn append(&self, data: Vec<u8>, policy: FsyncPolicy) -> anyhow::Result<()> {
        let (reply, response) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("store writer closed"))?
            .send(Append {
                data,
                policy,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("store writer stopped"))?;
        response
            .recv()
            .map_err(|_| anyhow::anyhow!("store writer stopped before acknowledgement"))?
            .map_err(anyhow::Error::msg)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Disconnect after the last producer is gone, drain queued records, join.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct TestSink {
        bytes: Vec<u8>,
        syncs: usize,
        fail_sync: bool,
    }
    impl Write for TestSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Sink for TestSink {
        fn sync(&mut self) -> std::io::Result<()> {
            self.syncs += 1;
            if self.fail_sync {
                Err(std::io::Error::other("sync failed"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn group_commit_syncs_once_and_propagates_sync_failure() {
        let (reply, _) = mpsc::sync_channel(1);
        let batch = (0..8)
            .map(|_| Append {
                data: b"{}\n".to_vec(),
                policy: FsyncPolicy::Always,
                reply: reply.clone(),
            })
            .collect::<Vec<_>>();
        let mut sink = TestSink::default();
        write_batch(&mut sink, &batch).unwrap();
        assert_eq!(sink.syncs, 1);
        assert_eq!(sink.bytes.len(), 24);
        sink.fail_sync = true;
        assert!(write_batch(&mut sink, &batch).is_err());
    }
    #[test]
    fn writer_open_failure_is_sticky_and_flush_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let writer = super::super::StoreWriter::new(Some(dir.path().into()));
        assert!(writer.append_json_value(&serde_json::json!({})).is_err());
        assert!(writer.flush().is_err());
    }

    #[test]
    fn bounded_queue_acknowledges_after_sync_and_drains_on_disconnect() {
        struct GatedSink {
            inner: TestSink,
            entered: SyncSender<()>,
            release: mpsc::Receiver<()>,
            gated: bool,
        }
        impl Write for GatedSink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.inner.write(bytes)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl Sink for GatedSink {
            fn sync(&mut self) -> std::io::Result<()> {
                if !self.gated {
                    self.gated = true;
                    self.entered
                        .send(())
                        .map_err(|_| std::io::Error::other("gate closed"))?;
                    self.release
                        .recv()
                        .map_err(|_| std::io::Error::other("gate closed"))?;
                }
                self.inner.sync()
            }
        }
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (entered, entered_rx) = mpsc::sync_channel(1);
        let (release, release_rx) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            run_writer(
                receiver,
                Ok(GatedSink {
                    inner: TestSink::default(),
                    entered,
                    release: release_rx,
                    gated: false,
                }),
            )
            .unwrap()
        });
        let (first_reply, first_response) = mpsc::sync_channel(1);
        sender
            .send(Append {
                data: b"{}\n".to_vec(),
                policy: FsyncPolicy::Always,
                reply: first_reply,
            })
            .unwrap();
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(
            first_response.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        let (reply, response) = mpsc::sync_channel(1);
        drop(response);
        for _ in 0..QUEUE_CAPACITY {
            sender
                .try_send(Append {
                    data: b"{}\n".to_vec(),
                    policy: FsyncPolicy::Always,
                    reply: reply.clone(),
                })
                .unwrap();
        }
        assert!(matches!(
            sender.try_send(Append {
                data: Vec::new(),
                policy: FsyncPolicy::Always,
                reply
            }),
            Err(mpsc::TrySendError::Full(_))
        ));
        drop(sender);
        release.send(()).unwrap();
        first_response
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let sink = thread.join().unwrap();
        assert_eq!(sink.inner.bytes.len(), (QUEUE_CAPACITY + 1) * 3);
        assert_eq!(sink.inner.syncs, 1 + QUEUE_CAPACITY / MAX_BATCH_RECORDS);
    }
}
