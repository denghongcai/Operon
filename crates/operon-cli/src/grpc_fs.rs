use std::{io::Write, path::Path};

use anyhow::Context;
use futures_util::StreamExt;
use operon_core::runtime::NodeEndpoint;
use operon_core::{FsList, FsStat, FsWrite};
use operon_grpc_client::chunk_write_requests;
use operon_protocol::runtime::v1::{
    write_file_request, FsCopyRequest, FsListRequest, FsPathRequest, FsRenameRequest,
    FsTruncateRequest, WriteFileRequest,
};

use crate::grpc::{call, with_auth, with_auth_stream, DEFAULT_LIST_PAGE_SIZE};

pub async fn fs_stat(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .stat_fs(with_auth(
                &endpoint,
                FsPathRequest {
                    path,
                    precondition: None,
                },
            )?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn fs_list(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsList> {
    let effective = crate::target::endpoint_with_overrides(endpoint.clone());
    let endpoint = &effective;
    let path = path.to_string();
    let mut entries = Vec::new();
    let mut page_token = String::new();
    let mut client = operon_grpc_client::connect(endpoint).await?;
    loop {
        let response = client
            .list_fs(with_auth(
                endpoint,
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
}

pub async fn read_file_to_writer(
    endpoint: &NodeEndpoint,
    path: &str,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        let mut stream = operon_grpc_client::bounded_rpc(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs),
            client.read_file(with_auth_stream(
                &endpoint,
                FsPathRequest {
                    path,
                    precondition: None,
                },
            )?),
        )
        .await?
        .into_inner();
        while let Some(chunk) = operon_grpc_client::bounded_rpc(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs),
            stream.message(),
        )
        .await?
        {
            writer.write_all(&chunk.data)?;
        }
        Ok(())
    })
    .await
}

pub async fn write_file_bytes(
    endpoint: &NodeEndpoint,
    path: &str,
    body: &[u8],
    expected_version: Option<String>,
) -> anyhow::Result<FsWrite> {
    let path = path.to_string();
    let chunks = chunk_write_requests(path, body, expected_version);
    call(endpoint, |mut client, endpoint| async move {
        let (progress, receiver) = tokio::sync::watch::channel(0_u64);
        let outbound = async_stream::stream! {
            for chunk in chunks {
                progress.send_modify(|count| *count += 1);
                yield chunk;
            }
        };
        let response = operon_grpc_client::transfer_progress(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs),
            receiver,
            client.write_file(operon_grpc_client::with_deadline(
                with_auth_stream(&endpoint, outbound)?,
                operon_core::runtime::TransportConfig::timeout(
                    endpoint.transport.transfer_timeout_secs,
                ),
            )),
        )
        .await?;
        Ok(response.into_inner().into())
    })
    .await
}

pub async fn write_file(
    endpoint: &NodeEndpoint,
    path: &str,
    file: &Path,
    expected_version: Option<String>,
) -> anyhow::Result<FsWrite> {
    let file = tokio::fs::File::open(file)
        .await
        .with_context(|| format!("failed to open {}", file.display()))?;
    write_reader(endpoint, path, file, expected_version).await
}

pub(crate) async fn write_reader(
    endpoint: &NodeEndpoint,
    path: &str,
    file: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    expected_version: Option<String>,
) -> anyhow::Result<FsWrite> {
    let target = chunk_write_requests(path.to_string(), &[], expected_version)
        .next()
        .ok_or_else(|| anyhow::anyhow!("write target metadata unavailable"))?;
    call(endpoint, |mut client, endpoint| async move {
        let mut source = tokio_util::io::ReaderStream::with_capacity(file, operon_grpc_client::STREAM_CHUNK_BYTES);
        let (read_error, read_failure) = tokio::sync::oneshot::channel();
        let (progress, receiver) = tokio::sync::watch::channel(0_u64);
        let outbound = async_stream::stream! {
            let mut read_error = Some(read_error);
            let mut sent_data = false;
            progress.send_modify(|count| *count += 1);
            yield target;
            while let Some(chunk) = source.next().await {
                match chunk {
                    Ok(data) => {
                        sent_data = true;
                        progress.send_modify(|count| *count += 1);
                        yield WriteFileRequest { payload: Some(write_file_request::Payload::Chunk(
                            operon_protocol::runtime::v1::FileChunk { data: data.to_vec() }
                        )) };
                    }
                    Err(error) => {
                        if let Some(sender) = read_error.take() { let _ = sender.send(error); }
                        // Never turn a local read failure into a successful
                        // truncated upload by producing clean request EOF.
                        std::future::pending::<()>().await;
                        return;
                    }
                }
            }
            if !sent_data {
                progress.send_modify(|count| *count += 1);
                yield WriteFileRequest { payload: Some(write_file_request::Payload::Chunk(
                    operon_protocol::runtime::v1::FileChunk { data: Vec::new() }
                )) };
            }
        };
        let rpc = operon_grpc_client::transfer_progress(
            std::time::Duration::from_secs(endpoint.transport.progress_timeout_secs), receiver,
            client.write_file(operon_grpc_client::with_deadline(
                with_auth_stream(&endpoint, outbound)?,
                operon_core::runtime::TransportConfig::timeout(endpoint.transport.transfer_timeout_secs),
            )),
        );
        let local_error = async {
            match read_failure.await {
                Ok(error) => error,
                Err(_) => std::future::pending::<std::io::Error>().await,
            }
        };
        tokio::select! {
            biased;
            error = local_error => Err(anyhow::Error::new(error).context("failed to read upload source")),
            result = rpc => Ok(result?.into_inner().into()),
        }
    }).await
}

pub async fn fs_mkdir(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<FsStat> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .mkdir_fs(with_auth(
                &endpoint,
                FsPathRequest {
                    path,
                    precondition: None,
                },
            )?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn fs_delete(endpoint: &NodeEndpoint, path: &str) -> anyhow::Result<String> {
    let path = path.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .delete_fs(with_auth(
                &endpoint,
                FsPathRequest {
                    path,
                    precondition: None,
                },
            )?)
            .await?
            .into_inner()
            .path)
    })
    .await
}

pub async fn fs_rename(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String)> {
    let request = FsRenameRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
        from_precondition: None,
        to_precondition: None,
        from_expected_version: None,
        to_expected_version: None,
        to_require_absent: false,
    };
    call(endpoint, |mut client, endpoint| async move {
        let response = client
            .rename_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path))
    })
    .await
}

pub async fn fs_copy(
    endpoint: &NodeEndpoint,
    from_path: &str,
    to_path: &str,
) -> anyhow::Result<(String, String, u64)> {
    let request = FsCopyRequest {
        from_path: from_path.to_string(),
        to_path: to_path.to_string(),
        from_precondition: None,
        to_precondition: None,
        from_expected_version: None,
        to_expected_version: None,
        to_require_absent: false,
    };
    call(endpoint, |mut client, endpoint| async move {
        let response = client
            .copy_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner();
        Ok((response.from_path, response.to_path, response.bytes_copied))
    })
    .await
}

pub async fn fs_truncate(endpoint: &NodeEndpoint, path: &str, size: u64) -> anyhow::Result<FsStat> {
    let request = FsTruncateRequest {
        path: path.to_string(),
        size,
        precondition: None,
        expected_version: None,
        require_absent: false,
    };
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .truncate_fs(with_auth(&endpoint, request)?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_write_requests_use_target_then_data_chunks() {
        let chunks = chunk_write_requests("file.txt".to_string(), &[1_u8; 70 * 1024], None)
            .collect::<Vec<_>>();

        assert_eq!(chunks.len(), 3);
        assert!(matches!(
            chunks[0].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Target(target)) if target.path == "file.txt"
        ));
        assert!(matches!(
            chunks[1].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 64 * 1024
        ));
        assert!(matches!(
            chunks[2].payload.as_ref(),
            Some(operon_protocol::runtime::v1::write_file_request::Payload::Chunk(chunk)) if chunk.data.len() == 6 * 1024
        ));
    }
}
