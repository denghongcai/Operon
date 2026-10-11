//! Bounded owned FUSE read tasks, separate from callback workers and RPC budgets.
use crate::remote_client::{block_on_runtime, GrpcRemoteFs};

#[derive(Default)]
pub(crate) struct ReadActivity {
    active: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
}
pub(crate) struct RpcActivity<'a>(&'a ReadActivity);
impl ReadActivity {
    pub(crate) fn start(&self) -> RpcActivity<'_> {
        use std::sync::atomic::Ordering::Relaxed;
        self.peak
            .fetch_max(self.active.fetch_add(1, Relaxed) + 1, Relaxed);
        RpcActivity(self)
    }
    pub(crate) fn snapshot(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering::Relaxed;
        (self.peak.load(Relaxed), self.active.load(Relaxed))
    }
}
impl Drop for RpcActivity<'_> {
    fn drop(&mut self) {
        self.0
            .active
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub(crate) struct ReadInterrupted;
impl std::fmt::Display for ReadInterrupted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FUSE read request interrupted")
    }
}
impl std::error::Error for ReadInterrupted {}

#[derive(Default)]
struct ReadRequests {
    active: Mutex<HashMap<fuser::RequestId, tokio::sync::watch::Sender<bool>>>,
}

impl ReadRequests {
    fn register(self: &Arc<Self>, id: fuser::RequestId) -> ReadCancellation {
        let (signal, receiver) = tokio::sync::watch::channel(false);
        self.active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id, signal.clone());
        ReadCancellation {
            requests: self.clone(),
            id,
            signal,
            receiver,
        }
    }

    fn interrupt(&self, id: fuser::RequestId) -> bool {
        let requests = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(signal) = requests.get(&id) {
            signal.send_replace(true);
            true
        } else {
            false
        }
    }
}

struct ReadCancellation {
    requests: Arc<ReadRequests>,
    id: fuser::RequestId,
    signal: tokio::sync::watch::Sender<bool>,
    receiver: tokio::sync::watch::Receiver<bool>,
}

impl ReadCancellation {
    async fn run<T>(&mut self, work: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
        tokio::select! {
            biased;
            _ = self.receiver.wait_for(|cancelled| *cancelled) => Err(ReadInterrupted.into()),
            result = work => result,
        }
    }
}

impl Drop for ReadCancellation {
    fn drop(&mut self) {
        let mut requests = self
            .requests
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Kernel IDs can be reused after a reply. An old task's destruction
        // must not remove a newer registration for the same numeric ID.
        if requests
            .get(&self.id)
            .is_some_and(|signal| signal.same_channel(&self.signal))
        {
            requests.remove(&self.id);
        }
    }
}
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

pub(crate) struct ReadTasks {
    slots: Arc<Semaphore>,
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl ReadTasks {
    fn new(limit: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(limit)),
            handles: Mutex::new(Vec::new()),
        }
    }

    #[cfg(test)]
    fn reserve(&self) -> Option<OwnedSemaphorePermit> {
        self.slots.clone().try_acquire_owned().ok()
    }

    async fn admit(
        &self,
        deadline: Option<tokio::time::Instant>,
    ) -> anyhow::Result<OwnedSemaphorePermit> {
        let acquire =
            async {
                self.slots.clone().acquire_owned().await.map_err(|_| {
                    tonic::Status::cancelled("mount read task admission closed").into()
                })
            };
        match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, acquire)
                .await
                .map_err(|_| {
                    tonic::Status::deadline_exceeded("mount read task admission deadline exceeded")
                })?,
            None => acquire.await,
        }
    }

    fn spawn(
        &self,
        runtime: &tokio::runtime::Runtime,
        task: impl Future<Output = ()> + Send + 'static,
    ) {
        let mut handles = self.handles.lock().unwrap_or_else(|e| e.into_inner());
        handles.retain(|task| !task.is_finished());
        handles.push(runtime.spawn(task));
    }

    fn cancel_and_join(&self, runtime: &tokio::runtime::Runtime) {
        self.slots.close();
        let handles = std::mem::take(&mut *self.handles.lock().unwrap_or_else(|e| e.into_inner()));
        for task in &handles {
            task.abort();
        }
        block_on_runtime(runtime, async {
            for task in handles {
                let _ = task.await;
            }
        });
    }
}

pub(crate) struct ReadScheduler {
    remote: Arc<GrpcRemoteFs>,
    tasks: ReadTasks,
    limit: usize,
    asynchronous: bool,
    requests: Arc<ReadRequests>,
}

impl ReadScheduler {
    pub(crate) fn new(remote: Arc<GrpcRemoteFs>, limit: usize, asynchronous: bool) -> Self {
        // Bounds queued + active tasks separately from active RPC/byte limits.
        Self {
            remote,
            tasks: ReadTasks::new(limit),
            limit,
            asynchronous,
            requests: Arc::new(ReadRequests::default()),
        }
    }

    pub(crate) fn interrupt(&self, id: fuser::RequestId) -> bool {
        self.requests.interrupt(id)
    }

    pub(crate) fn dispatch(
        &self,
        id: fuser::RequestId,
        path: String,
        offset: u64,
        size: u32,
        reply: fuser::ReplyData,
    ) {
        let deadline = self.remote.read_deadline();
        let path = match crate::mount_core::normalize_remote_path(&path) {
            Ok(path) => path,
            Err(error) => {
                reply.error(crate::errors::errno_for_error(&error));
                return;
            }
        };
        let Ok(runtime) = self.remote.runtime() else {
            reply.error(fuser::Errno::EIO);
            return;
        };
        let mut cancellation = self.requests.register(id);
        if !self.asynchronous {
            let result = block_on_runtime(
                runtime,
                cancellation.run(
                    self.remote
                        .read_range_with_deadline(&path, offset, size, deadline),
                ),
            );
            drop(cancellation);
            match result {
                Ok(data) => reply.data(&data),
                Err(error) => reply.error(crate::errors::errno_for_error(&error)),
            }
            return;
        }
        let admission = block_on_runtime(
            runtime,
            cancellation.run(self.remote.until_shutdown(self.tasks.admit(deadline))),
        );
        let slot = match admission {
            Ok(slot) => slot,
            Err(error) => {
                drop(cancellation);
                reply.error(crate::errors::errno_for_error(&error));
                return;
            }
        };
        let remote = self.remote.clone();
        self.tasks.spawn(runtime, async move {
            let _slot = slot;
            let result = cancellation
                .run(remote.read_range_with_deadline(&path, offset, size, deadline))
                .await;
            drop(cancellation);
            match result {
                Ok(data) => reply.data(&data),
                Err(error) => reply.error(crate::errors::errno_for_error(&error)),
            }
        });
    }
}

impl Drop for ReadScheduler {
    fn drop(&mut self) {
        if let Ok(runtime) = self.remote.runtime() {
            // Cancel futures and await their destruction BEFORE dropping the
            // remote runtime. Dropped ReplyData sends one EIO via fuser RAII.
            self.tasks.cancel_and_join(runtime);
            let (peak, active) = self.remote.read_activity.snapshot();
            eprintln!(
                "operon {} read: peak_rpc={} active_after_join={} task_limit={}",
                if self.asynchronous { "async" } else { "sync" },
                peak,
                active,
                self.limit,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interrupt_cancels_only_target_and_removes_finished_ids() {
        let requests = Arc::new(ReadRequests::default());
        let mut first = requests.register(fuser::RequestId(1));
        let second = requests.register(fuser::RequestId(2));
        assert!(!requests.interrupt(fuser::RequestId(999)));
        assert!(requests.interrupt(fuser::RequestId(1)));
        let error = first
            .run(std::future::pending::<anyhow::Result<()>>())
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<ReadInterrupted>().is_some());
        assert_eq!(
            format!("{:?}", crate::errors::errno_for_error(&error)),
            format!("{:?}", fuser::Errno::EINTR)
        );
        assert!(!*second.receiver.borrow());
        drop(first);
        assert!(!requests.interrupt(fuser::RequestId(1)));
        drop(second);
        assert!(requests.active.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn interrupt_cancels_task_admission_without_consuming_a_slot() {
        let tasks = ReadTasks::new(1);
        let occupied = tasks.reserve().unwrap();
        let requests = Arc::new(ReadRequests::default());
        let mut cancellation = requests.register(fuser::RequestId(1));
        assert!(requests.interrupt(fuser::RequestId(1)));
        let error = cancellation.run(tasks.admit(None)).await.unwrap_err();
        assert!(error.downcast_ref::<ReadInterrupted>().is_some());
        assert_eq!(tasks.slots.available_permits(), 0);
        drop(cancellation);
        drop(occupied);
        assert_eq!(tasks.slots.available_permits(), 1);
    }

    #[test]
    fn reused_request_id_cannot_be_removed_by_an_old_guard() {
        let requests = Arc::new(ReadRequests::default());
        let old = requests.register(fuser::RequestId(1));
        let new = requests.register(fuser::RequestId(1));
        drop(old);
        assert!(requests.interrupt(fuser::RequestId(1)));
        assert!(*new.receiver.borrow());
        drop(new);
        assert!(!requests.interrupt(fuser::RequestId(1)));
    }

    #[test]
    fn rpc_activity_is_per_mount_and_released_by_guards() {
        let first = ReadActivity::default();
        let second = ReadActivity::default();
        let one = first.start();
        let two = first.start();
        let other = second.start();
        assert_eq!(first.snapshot(), (2, 2));
        drop(one);
        drop(two);
        assert_eq!(first.snapshot(), (2, 0));
        assert_eq!(second.snapshot(), (1, 1));
        drop(other);
        assert_eq!(second.snapshot(), (1, 0));
    }

    #[tokio::test]
    async fn admission_deadline_and_closed_queue_do_not_leak_slots() {
        let tasks = ReadTasks::new(1);
        let slot = tasks.reserve().unwrap();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(10);
        let error = tasks.admit(Some(deadline)).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<tonic::Status>().unwrap().code(),
            tonic::Code::DeadlineExceeded
        );
        assert_eq!(tasks.slots.available_permits(), 0);
        tasks.slots.close();
        let error = tasks.admit(None).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<tonic::Status>().unwrap().code(),
            tonic::Code::Cancelled
        );
        drop(slot);
        assert_eq!(tasks.slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn admission_does_not_restart_an_expired_deadline() {
        let tasks = ReadTasks::new(1);
        // Consume the sole permit so an already-expired deadline must win.
        let _slot = tasks.reserve().unwrap();
        let deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
        let error = tasks.admit(Some(deadline)).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<tonic::Status>().unwrap().code(),
            tonic::Code::DeadlineExceeded
        );
    }
    #[test]
    fn completed_tasks_release_slots_and_can_be_reused() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tasks = ReadTasks::new(1);
        for _ in 0..8 {
            let slot = tasks.reserve().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            tasks.spawn(&runtime, async move {
                drop(slot);
                tx.send(()).unwrap();
            });
            rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        }
        tasks.cancel_and_join(&runtime);
        assert_eq!(tasks.slots.available_permits(), 1);
    }

    #[test]
    fn bounded_tasks_cancel_join_and_release_all_slots() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tasks = ReadTasks::new(2);
        for _ in 0..2 {
            let slot = tasks.reserve().unwrap();
            tasks.spawn(&runtime, async move {
                let _slot = slot;
                std::future::pending::<()>().await;
            });
        }
        assert!(tasks.reserve().is_none());
        tasks.cancel_and_join(&runtime);
        assert_eq!(tasks.slots.available_permits(), 2);
        assert!(tasks.reserve().is_none());
        assert!(tasks.handles.lock().unwrap().is_empty());
    }
}
