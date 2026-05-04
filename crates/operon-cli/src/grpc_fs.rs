use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

use anyhow::Context;
use futures_util::stream;
use operon_core::{FsList, FsStat, FsWrite};
use operon_grpc_client::chunk_write_requests;
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    FsCopyRequest, FsListRequest, FsPathRequest, FsRenameRequest, FsTruncateRequest,
};

use crate::grpc::{call, with_auth, DEFAULT_LIST_PAGE_SIZE};

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
    let path = path.to_string();
    let mut entries = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let path = path.clone();
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_fs(with_auth(
                        &endpoint,
                        FsListRequest {
                            path,
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
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
        let mut stream = client
            .read_file(with_auth(
                &endpoint,
                FsPathRequest {
                    path,
                    precondition: None,
                },
            )?)
            .await?
            .into_inner();
        while let Some(chunk) = stream.message().await? {
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
        Ok(client
            .write_file(with_auth(&endpoint, stream::iter(chunks))?)
            .await?
            .into_inner()
            .into())
    })
    .await
}

pub async fn write_file(
    endpoint: &NodeEndpoint,
    path: &str,
    file: &Path,
    expected_version: Option<String>,
) -> anyhow::Result<FsWrite> {
    let mut data = Vec::new();
    fs::File::open(file)
        .with_context(|| format!("failed to open {}", file.display()))?
        .read_to_end(&mut data)?;
    write_file_bytes(endpoint, path, &data, expected_version).await
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
        let chunks = chunk_write_requests("file.txt".to_string(), &[1_u8; 70 * 1024], None);

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
