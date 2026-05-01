use std::sync::{Mutex, MutexGuard};

use tonic::Status;

pub(crate) fn lock<'a, T>(
    mutex: &'a Mutex<T>,
    name: &'static str,
) -> Result<MutexGuard<'a, T>, Status> {
    mutex
        .lock()
        .map_err(|_| Status::internal(format!("{name} mutex poisoned")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn poisoned_lock_returns_internal_status() {
        let mutex = Arc::new(Mutex::new(0));
        let worker_mutex = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = worker_mutex.lock().expect("initial lock");
            panic!("poison lock");
        })
        .join();

        let status = lock(&mutex, "test").expect_err("lock should be poisoned");
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("test mutex poisoned"));
    }
}
