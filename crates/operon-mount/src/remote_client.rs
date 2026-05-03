use std::{future::Future, panic};

use operon_core::{FsList, FsStat};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    operon_runtime_client::OperonRuntimeClient, FsListRequest, FsPathRequest, FsReadRangeRequest,
    FsRenameRequest, FsTruncateRequest, FsWriteRangeRequest,
};
use tonic::transport::Channel;

const DEFAULT_LIST_PAGE_SIZE: u32 = 1000;

pub trait RemoteFs: Send + Sync {
    fn stat(&self, path: &str) -> anyhow::Result<FsStat>;
    fn list(&self, path: &str) -> anyhow::Result<FsList>;
    fn read_range(&self, path: &str, offset: u64, size: u32) -> anyhow::Result<Vec<u8>>;
    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64>;
    fn truncate(&self, path: &str, size: u64) -> anyhow::Result<FsStat>;
    fn mkdir(&self, path: &str) -> anyhow::Result<FsStat>;
    fn delete(&self, path: &str) -> anyhow::Result<()>;
    fn rename(&self, from_path: &str, to_path: &str) -> anyhow::Result<()>;
}

pub(crate) struct GrpcRemoteFs {
    endpoint: NodeEndpoint,
    channel: Channel,
    runtime: Option<tokio::runtime::Runtime>,
}

impl GrpcRemoteFs {
    pub(crate) fn connect(endpoint: NodeEndpoint) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let uri = operon_grpc_client::grpc_channel_uri(&endpoint.endpoint)?;
        let builder = Channel::from_shared(uri)?;
        let channel = block_on_runtime(&runtime, async { builder.connect().await })?;
        Ok(Self {
            endpoint,
            channel,
            runtime: Some(runtime),
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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
        let request = FsReadRangeRequest {
            path: path.to_string(),
            offset,
            size,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .read_file_range(operon_grpc_client::request(&self.endpoint, request)?)
                .await?
                .into_inner()
                .data)
        })
    }

    fn write_range(&self, path: &str, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        let request = FsWriteRangeRequest {
            path: path.to_string(),
            offset,
            data: data.to_vec(),
            precondition: None,
            expected_version: None,
            require_absent: false,
        };
        block_on_runtime(self.runtime()?, async {
            let mut client = OperonRuntimeClient::new(self.channel.clone());
            Ok(client
                .write_file_range(operon_grpc_client::request(&self.endpoint, request)?)
                .await?
                .into_inner()
                .bytes_written)
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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
            let mut client = OperonRuntimeClient::new(self.channel.clone());
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

    std::thread::scope(
        |scope| match scope.spawn(|| runtime.block_on(future)).join() {
            Ok(result) => result,
            Err(payload) => panic::resume_unwind(payload),
        },
    )
}
