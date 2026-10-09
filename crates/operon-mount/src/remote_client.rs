use std::{future::Future, panic};

use operon_core::runtime::NodeEndpoint;
use operon_core::{FsList, FsStat, FsWrite};
use operon_protocol::runtime::v1::{
    FsListRequest, FsPathRequest, FsReadRangeRequest, FsRenameRequest, FsTruncateRequest,
    FsWriteRangeRequest,
};
use tonic::transport::Channel;

use crate::mount_core::RemoteFs;

const DEFAULT_LIST_PAGE_SIZE: u32 = 1000;

pub struct GrpcRemoteFs {
    endpoint: NodeEndpoint,
    channel: Channel,
    runtime: Option<tokio::runtime::Runtime>,
    read_requests: tokio::sync::Semaphore,
    read_bytes: tokio::sync::Semaphore,
}

impl GrpcRemoteFs {
    pub fn connect(endpoint: NodeEndpoint) -> anyhow::Result<Self> {
        Self::connect_with_reads(endpoint, Default::default())
    }

    pub fn connect_with_reads(
        endpoint: NodeEndpoint,
        reads: operon_core::runtime::MountReadConfig,
    ) -> anyhow::Result<Self> {
        reads.validate().map_err(anyhow::Error::msg)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let connected = block_on_runtime(&runtime, async {
            operon_grpc_client::connect_channel(
                &endpoint,
                std::time::Duration::from_secs(endpoint.transport.connect_timeout_secs),
            )
            .await
        });
        let channel = match connected {
            Ok(channel) => channel,
            Err(error) => {
                // Runtime::drop panics inside an async caller. Failed setup
                // must use the same nonblocking shutdown discipline as Drop.
                runtime.shutdown_background();
                return Err(error);
            }
        };
        Ok(Self {
            endpoint,
            channel,
            runtime: Some(runtime),
            read_requests: tokio::sync::Semaphore::new(reads.max_inflight_reads),
            read_bytes: tokio::sync::Semaphore::new(reads.max_inflight_read_mib as usize * 1024),
        })
    }

    fn runtime(&self) -> anyhow::Result<&tokio::runtime::Runtime> {
        self.runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("remote fs runtime is unavailable"))
    }
}

impl RemoteFs for GrpcRemoteFs {
    fn stat(&self, path: &str) -> anyhow::Result<FsStat> {
        let path = path.to_string();
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            Ok(client
                .stat_fs(operon_grpc_client::request(
                    &self.endpoint,
                    FsPathRequest {
                        path,
                        precondition: None,
                    },
                )?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<FsList> {
        let path = path.to_string();
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            let mut entries = Vec::new();
            let mut page_token = String::new();
            loop {
                let response = client
                    .list_fs(operon_grpc_client::request(
                        &self.endpoint,
                        FsListRequest {
                            path: path.clone(),
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner();
                entries.extend(response.entries.into_iter().map(Into::into));
                if response.next_page_token.is_empty() {
                    break;
                }
                page_token = response.next_page_token;
            }
            Ok(FsList {
                path,
                entries,
                next_page_token: String::new(),
            })
        })
    }

    fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(size <= 8 * 1024 * 1024, "read range exceeds 8 MiB");
        let request = FsReadRangeRequest {
            path: path.to_string(),
            offset,
            size,
        };
        block_on_runtime(self.runtime()?, async {
            let read = async {
                let _request = self.read_requests.acquire().await?;
                // Conservative KiB accounting also fits Semaphore::MAX_PERMITS
                // on supported 32-bit ARMv7, including the 512 MiB setting.
                let _bytes = self.read_bytes.acquire_many(size.div_ceil(1024)).await?;
                let mut client = operon_grpc_client::runtime_client(self.channel.clone());
                Ok(client
                    .read_file_range(operon_grpc_client::request(&self.endpoint, request)?)
                    .await?
                    .into_inner()
                    .data)
            };
            match operon_core::runtime::TransportConfig::timeout(
                self.endpoint.transport.rpc_timeout_secs,
            ) {
                Some(timeout) => tokio::time::timeout(timeout, read).await.map_err(|_| {
                    tonic::Status::deadline_exceeded("mount read queue/RPC deadline exceeded")
                })?,
                None => read.await,
            }
        })
    }

    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        Ok(self
            .write_range_with_stat(path, offset, data)?
            .bytes_written)
    }

    fn write_range_with_stat(
        &self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> anyhow::Result<FsWrite> {
        let request = FsWriteRangeRequest {
            path: path.to_string(),
            offset,
            data: data.to_vec(),
            precondition: None,
            expected_version: None,
            require_absent: false,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            Ok(client
                .write_file_range(operon_grpc_client::request(&self.endpoint, request)?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat> {
        let request = FsTruncateRequest {
            path: path.to_string(),
            size,
            precondition: None,
            expected_version: None,
            require_absent: false,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            Ok(client
                .truncate_fs(operon_grpc_client::request(&self.endpoint, request)?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn mkdir(&self, path: &str) -> anyhow::Result<FsStat> {
        let request = FsPathRequest {
            path: path.to_string(),
            precondition: None,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            Ok(client
                .mkdir_fs(operon_grpc_client::request(&self.endpoint, request)?)
                .await?
                .into_inner()
                .into())
        })
    }

    fn delete(&self, path: &str) -> anyhow::Result<()> {
        let request = FsPathRequest {
            path: path.to_string(),
            precondition: None,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            client
                .delete_fs(operon_grpc_client::request(&self.endpoint, request)?)
                .await?;
            Ok(())
        })
    }

    fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()> {
        let request = FsRenameRequest {
            from_path: from_path.to_string(),
            to_path: to_path.to_string(),
            from_precondition: None,
            to_precondition: None,
            from_expected_version: None,
            to_expected_version: None,
            to_require_absent: false,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = operon_grpc_client::runtime_client(self.channel.clone());
            client
                .rename_fs(operon_grpc_client::request(&self.endpoint, request)?)
                .await?;
            Ok(())
        })
    }
}

impl Drop for GrpcRemoteFs {
    fn drop(&mut self) {
        let Some(runtime) = self.runtime.take() else {
            return;
        };

        if tokio::runtime::Handle::try_current().is_err() {
            drop(runtime);
            return;
        }

        match std::thread::spawn(move || drop(runtime)).join() {
            Ok(()) => {}
            Err(payload) => panic::resume_unwind(payload),
        }
    }
}

fn block_on_runtime<F, T>(runtime: &tokio::runtime::Runtime, future: F) -> T
where
    F: Future<Output = T> + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_err() {
        return runtime.block_on(future);
    }

    // The transport runtime owns continuously running reactor workers. Enter
    // it while polling on this synchronous caller instead of starting an OS
    // thread per nested call. Async callers should use spawn_blocking for this
    // inherently synchronous API, especially on a current-thread parent runtime.
    struct WakeThread(std::thread::Thread);
    impl std::task::Wake for WakeThread {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let _entered = runtime.enter();
    let waker = std::task::Waker::from(std::sync::Arc::new(WakeThread(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(result) => return result,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_read_budget_fits_32bit_semaphore_limit() {
        let largest = 512_usize * 1024;
        assert!(largest <= (u32::MAX >> 3) as usize);
        let semaphore = tokio::sync::Semaphore::new(largest);
        assert_eq!(semaphore.available_permits(), largest);
        assert_eq!(0_u32.div_ceil(1024), 0);
        assert_eq!(1025_u32.div_ceil(1024), 2);
        assert_eq!((8_u32 * 1024 * 1024).div_ceil(1024), 8192);
    }

    #[test]
    fn read_queue_deadline_releases_request_and_byte_permits() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let channel = runtime.block_on(async {
            tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy()
        });
        let remote = GrpcRemoteFs {
            endpoint: NodeEndpoint {
                node_id: "test".into(),
                endpoint: "grpc://127.0.0.1:1".into(),
                token: None,
                transport: operon_core::runtime::TransportConfig {
                    rpc_timeout_secs: 1,
                    ..Default::default()
                },
            },
            channel,
            runtime: Some(runtime),
            read_requests: tokio::sync::Semaphore::new(1),
            read_bytes: tokio::sync::Semaphore::new(8 * 1024),
        };
        {
            let _bytes = remote.read_bytes.try_acquire_many(8 * 1024).unwrap();
            let error = remote.read_range("/file", 0, 1).unwrap_err();
            assert_eq!(
                error.downcast_ref::<tonic::Status>().unwrap().code(),
                tonic::Code::DeadlineExceeded
            );
            assert_eq!(remote.read_requests.available_permits(), 1);
            assert_eq!(remote.read_bytes.available_permits(), 0);
        }
        assert_eq!(remote.read_bytes.available_permits(), 8 * 1024);
        {
            let _request = remote.read_requests.try_acquire().unwrap();
            assert!(remote.read_range("/file", 0, 1).is_err());
            assert_eq!(remote.read_bytes.available_permits(), 8 * 1024);
        }
        assert_eq!(remote.read_requests.available_permits(), 1);
        assert!(remote.read_range("/file", 0, 8 * 1024 * 1024 + 1).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_connect_inside_async_runtime_returns_error_without_drop_panic() {
        let endpoint = NodeEndpoint {
            node_id: "unreachable".into(),
            endpoint: "grpc://127.0.0.1:0".into(),
            token: None,
            transport: operon_core::runtime::TransportConfig {
                connect_timeout_secs: 1,
                ..Default::default()
            },
        };
        assert!(GrpcRemoteFs::connect(endpoint).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn nested_bridge_drives_timers_without_a_per_call_thread() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let caller = std::thread::current().id();
        for value in 0..8 {
            let borrowed = &value;
            assert_eq!(
                block_on_runtime(&runtime, async {
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    assert_eq!(std::thread::current().id(), caller);
                    tokio::spawn(async { 42 }).await.unwrap() + *borrowed
                }),
                42 + value
            );
        }
        runtime.shutdown_background();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shared_runtime_supports_concurrent_nested_bridges_and_cancellation() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let parent = tokio::runtime::Handle::current();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    let runtime = &runtime;
                    let parent = &parent;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        let _entered = parent.enter();
                        let caller = std::thread::current().id();
                        barrier.wait();
                        for _ in 0..20 {
                            block_on_runtime(runtime, async {
                                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                                assert_eq!(std::thread::current().id(), caller);
                                assert!(operon_grpc_client::bounded_rpc(
                                    std::time::Duration::from_millis(1),
                                    std::future::pending::<Result<(), tonic::Status>>(),
                                )
                                .await
                                .is_err());
                            });
                        }
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                handle.join().unwrap();
            }
        });
        runtime.shutdown_background();
    }
}
