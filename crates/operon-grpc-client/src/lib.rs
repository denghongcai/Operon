use std::{future::Future, time::Duration};

use anyhow::Context;
use operon_core::RequestContext;
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    exec_stdin_request, operon_runtime_client::OperonRuntimeClient, write_file_request,
    ExecStdinRequest, ExecStdinTarget, FileChunk, FsPrecondition, WriteFileRequest,
    WriteFileTarget,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};

pub const RUN_ID_METADATA: &str = "x-operon-run-id";
pub const STEP_ID_METADATA: &str = "x-operon-step-id";
pub const STREAM_CHUNK_BYTES: usize = 64 * 1024;
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn grpc_channel_uri(endpoint: &str) -> anyhow::Result<String> {
    if let Some(rest) = endpoint.strip_prefix("grpc://") {
        Ok(format!("http://{rest}"))
    } else if let Some(rest) = endpoint.strip_prefix("grpcs://") {
        Ok(format!("https://{rest}"))
    } else {
        anyhow::bail!("only grpc:// and grpcs:// endpoints are supported")
    }
}

pub async fn connect(endpoint: &NodeEndpoint) -> anyhow::Result<OperonRuntimeClient<Channel>> {
    Ok(OperonRuntimeClient::new(
        connect_channel(endpoint, DEFAULT_CONNECT_TIMEOUT).await?,
    ))
}

pub async fn connect_channel(
    endpoint: &NodeEndpoint,
    timeout: Duration,
) -> anyhow::Result<Channel> {
    let channel = Channel::from_shared(grpc_channel_uri(&endpoint.endpoint)?)?;
    with_connect_timeout(&endpoint.endpoint, timeout, async {
        channel
            .connect()
            .await
            .with_context(|| format!("failed to connect to {}", endpoint.endpoint))
    })
    .await
}

async fn with_connect_timeout<T, F>(
    endpoint: &str,
    timeout: Duration,
    future: F,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| anyhow::anyhow!("gRPC connection to {endpoint} timed out after {timeout:?}"))?
}

pub fn request<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<Request<T>> {
    request_with_context(endpoint, None, message)
}

pub fn request_with_context<T>(
    endpoint: &NodeEndpoint,
    context: Option<&RequestContext>,
    message: T,
) -> anyhow::Result<Request<T>> {
    let mut request = Request::new(message);
    apply_metadata(endpoint, context, request.metadata_mut())?;
    Ok(request)
}

pub fn apply_metadata(
    endpoint: &NodeEndpoint,
    context: Option<&RequestContext>,
    metadata: &mut tonic::metadata::MetadataMap,
) -> anyhow::Result<()> {
    if let Some(token) = &endpoint.token {
        metadata.insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))?,
        );
    }
    if let Some(context) = context {
        if let Some(run_id) = &context.run_id {
            metadata.insert(RUN_ID_METADATA, MetadataValue::try_from(run_id.as_str())?);
        }
        if let Some(step_id) = &context.step_id {
            metadata.insert(STEP_ID_METADATA, MetadataValue::try_from(step_id.as_str())?);
        }
    }
    Ok(())
}

pub fn chunk_write_requests(
    path: String,
    body: &[u8],
    expected_version: Option<String>,
) -> Vec<WriteFileRequest> {
    let expected_version_for_precondition = expected_version.clone();
    let mut chunks = vec![WriteFileRequest {
        payload: Some(write_file_request::Payload::Target(WriteFileTarget {
            path,
            precondition: expected_version_for_precondition.map(|expected_version| {
                FsPrecondition {
                    expected_version: Some(expected_version),
                    require_absent: false,
                }
            }),
            expected_version,
            require_absent: false,
        })),
    }];
    if body.is_empty() {
        chunks.push(WriteFileRequest {
            payload: Some(write_file_request::Payload::Chunk(FileChunk {
                data: Vec::new(),
            })),
        });
        return chunks;
    }
    chunks.extend(
        body.chunks(STREAM_CHUNK_BYTES)
            .map(|chunk| WriteFileRequest {
                payload: Some(write_file_request::Payload::Chunk(FileChunk {
                    data: chunk.to_vec(),
                })),
            }),
    );
    chunks
}

pub fn chunk_stdin_requests(exec_id: String, body: &[u8]) -> Vec<ExecStdinRequest> {
    let mut chunks = vec![ExecStdinRequest {
        payload: Some(exec_stdin_request::Payload::Target(ExecStdinTarget {
            exec_id,
        })),
    }];
    if body.is_empty() {
        chunks.push(ExecStdinRequest {
            payload: Some(exec_stdin_request::Payload::Chunk(FileChunk {
                data: Vec::new(),
            })),
        });
        return chunks;
    }
    chunks.extend(
        body.chunks(STREAM_CHUNK_BYTES)
            .map(|chunk| ExecStdinRequest {
                payload: Some(exec_stdin_request::Payload::Chunk(FileChunk {
                    data: chunk.to_vec(),
                })),
            }),
    );
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_protocol::runtime::v1::{exec_stdin_request, write_file_request};

    #[test]
    fn converts_operon_grpc_uris_to_tonic_uris() {
        assert_eq!(
            grpc_channel_uri("grpc://127.0.0.1:7789").expect("grpc uri"),
            "http://127.0.0.1:7789"
        );
        assert_eq!(
            grpc_channel_uri("grpcs://node.example:7789").expect("grpcs uri"),
            "https://node.example:7789"
        );
        assert!(grpc_channel_uri("http://127.0.0.1:7789").is_err());
    }

    #[test]
    fn request_includes_auth_and_execution_context_metadata() {
        let endpoint = NodeEndpoint {
            node_id: "local".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            token: Some("token".to_string()),
        };
        let context = RequestContext {
            run_id: Some("run-1".to_string()),
            step_id: Some("step-1".to_string()),
        };
        let request = request_with_context(&endpoint, Some(&context), ()).expect("request");
        assert_eq!(
            request
                .metadata()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token")
        );
        assert_eq!(
            request
                .metadata()
                .get(RUN_ID_METADATA)
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        assert_eq!(
            request
                .metadata()
                .get(STEP_ID_METADATA)
                .and_then(|value| value.to_str().ok()),
            Some("step-1")
        );
    }

    #[test]
    fn chunks_empty_streams_with_explicit_empty_chunk() {
        assert_eq!(
            chunk_write_requests("/empty".to_string(), &[], None).len(),
            2
        );
        assert_eq!(chunk_stdin_requests("exec-1".to_string(), &[]).len(), 2);
    }

    #[test]
    fn chunks_write_target_can_include_expected_version() {
        let chunks = chunk_write_requests(
            "/file.txt".to_string(),
            &[1, 2, 3],
            Some("version-1".to_string()),
        );
        let target = chunks
            .first()
            .and_then(|chunk| chunk.payload.as_ref())
            .expect("target payload");
        let write_file_request::Payload::Target(target) = target else {
            panic!("first write request should carry target metadata");
        };
        assert_eq!(target.expected_version.as_deref(), Some("version-1"));
    }

    #[test]
    fn request_without_auth_or_context_leaves_operon_metadata_empty() {
        let endpoint = NodeEndpoint {
            node_id: "local".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            token: None,
        };

        let request = request(&endpoint, ()).expect("request");

        assert!(request.metadata().get("authorization").is_none());
        assert!(request.metadata().get(RUN_ID_METADATA).is_none());
        assert!(request.metadata().get(STEP_ID_METADATA).is_none());
    }

    #[test]
    fn request_metadata_allows_partial_execution_context() {
        let endpoint = NodeEndpoint {
            node_id: "local".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            token: None,
        };
        let run_only = RequestContext {
            run_id: Some("run-1".to_string()),
            step_id: None,
        };
        let step_only = RequestContext {
            run_id: None,
            step_id: Some("step-1".to_string()),
        };

        let run_request =
            request_with_context(&endpoint, Some(&run_only), ()).expect("run request");
        let step_request =
            request_with_context(&endpoint, Some(&step_only), ()).expect("step request");

        assert_eq!(
            run_request
                .metadata()
                .get(RUN_ID_METADATA)
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        assert!(run_request.metadata().get(STEP_ID_METADATA).is_none());
        assert!(step_request.metadata().get(RUN_ID_METADATA).is_none());
        assert_eq!(
            step_request
                .metadata()
                .get(STEP_ID_METADATA)
                .and_then(|value| value.to_str().ok()),
            Some("step-1")
        );
    }

    #[test]
    fn chunks_non_empty_stdin_streams_at_configured_boundary() {
        let body = vec![7_u8; STREAM_CHUNK_BYTES + 3];

        let chunks = chunk_stdin_requests("exec-1".to_string(), &body);

        assert_eq!(chunks.len(), 3);
        let exec_stdin_request::Payload::Target(target) =
            chunks[0].payload.as_ref().expect("target")
        else {
            panic!("first stdin request should carry target");
        };
        assert_eq!(target.exec_id, "exec-1");
        let exec_stdin_request::Payload::Chunk(first) =
            chunks[1].payload.as_ref().expect("first chunk")
        else {
            panic!("second stdin request should carry chunk");
        };
        let exec_stdin_request::Payload::Chunk(second) =
            chunks[2].payload.as_ref().expect("second chunk")
        else {
            panic!("third stdin request should carry chunk");
        };
        assert_eq!(first.data.len(), STREAM_CHUNK_BYTES);
        assert_eq!(second.data, vec![7_u8; 3]);
    }

    #[test]
    fn chunks_non_empty_write_streams_at_configured_boundary() {
        let body = vec![9_u8; STREAM_CHUNK_BYTES * 2 + 1];

        let chunks = chunk_write_requests("/file.bin".to_string(), &body, None);

        assert_eq!(chunks.len(), 4);
        let write_file_request::Payload::Target(target) =
            chunks[0].payload.as_ref().expect("target")
        else {
            panic!("first write request should carry target");
        };
        assert_eq!(target.path, "/file.bin");
        let sizes = chunks
            .iter()
            .skip(1)
            .map(|chunk| match chunk.payload.as_ref().expect("chunk") {
                write_file_request::Payload::Chunk(chunk) => chunk.data.len(),
                write_file_request::Payload::Target(_) => panic!("unexpected target after first"),
            })
            .collect::<Vec<_>>();
        assert_eq!(sizes, vec![STREAM_CHUNK_BYTES, STREAM_CHUNK_BYTES, 1]);
    }

    #[tokio::test]
    async fn connect_deadline_wraps_pending_connection_future() {
        let error = with_connect_timeout(
            "grpc://127.0.0.1:7789",
            std::time::Duration::from_millis(10),
            async { std::future::pending::<anyhow::Result<()>>().await },
        )
        .await
        .expect_err("unresponsive endpoint should time out");

        assert!(error.to_string().contains("timed out"));
    }
}
