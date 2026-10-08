//! Shared gRPC transport primitives.
//!
//! This crate owns channels, metadata, authentication, timeouts, and request
//! chunking. Domain-level RPC orchestration remains with each application.

use std::{future::Future, time::Duration};

use anyhow::Context;
use operon_core::runtime::{NodeEndpoint, RequestContext};
use operon_protocol::runtime::v1::{
    exec_stdin_request, operon_runtime_client::OperonRuntimeClient, write_file_request,
    ExecStdinRequest, ExecStdinTarget, FileChunk, FsPrecondition, WriteFileRequest,
    WriteFileTarget,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};

mod deadline_channel;
pub use deadline_channel::DeadlineChannel;

pub const RUN_ID_METADATA: &str = "x-operon-run-id";
pub const STEP_ID_METADATA: &str = "x-operon-step-id";
pub const STREAM_CHUNK_BYTES: usize = 64 * 1024;
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);
pub const DEFAULT_PROGRESS_TIMEOUT: Duration = Duration::from_secs(60);
pub use operon_protocol::{KEEPALIVE_INTERVAL, KEEPALIVE_TIMEOUT};

pub fn grpc_channel_uri(endpoint: &str) -> anyhow::Result<String> {
    if let Some(rest) = endpoint.strip_prefix("grpc://") {
        Ok(format!("http://{rest}"))
    } else if let Some(rest) = endpoint.strip_prefix("grpcs://") {
        Ok(format!("https://{rest}"))
    } else {
        anyhow::bail!("only grpc:// and grpcs:// endpoints are supported")
    }
}

pub async fn connect(
    endpoint: &NodeEndpoint,
) -> anyhow::Result<OperonRuntimeClient<DeadlineChannel>> {
    Ok(runtime_client(
        connect_channel(
            endpoint,
            Duration::from_secs(endpoint.transport.connect_timeout_secs),
        )
        .await?,
    ))
}

pub fn runtime_client(channel: Channel) -> OperonRuntimeClient<DeadlineChannel> {
    OperonRuntimeClient::new(DeadlineChannel(channel))
        .max_decoding_message_size(operon_protocol::MAX_GRPC_MESSAGE_BYTES)
        .max_encoding_message_size(operon_protocol::MAX_GRPC_MESSAGE_BYTES)
}

pub async fn connect_channel(
    endpoint: &NodeEndpoint,
    timeout: Duration,
) -> anyhow::Result<Channel> {
    let config = &endpoint.transport;
    config.validate().map_err(anyhow::Error::msg)?;
    let mut channel = Channel::from_shared(grpc_channel_uri(&endpoint.endpoint)?)?
        .keep_alive_while_idle(config.keepalive_while_idle)
        .http2_adaptive_window(config.adaptive_window);
    if let Some(interval) =
        operon_core::runtime::TransportConfig::timeout(config.keepalive_interval_secs)
    {
        channel = channel.http2_keep_alive_interval(interval);
    }
    if let Some(timeout) =
        operon_core::runtime::TransportConfig::timeout(config.keepalive_timeout_secs)
    {
        channel = channel.keep_alive_timeout(timeout);
    }
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
    if timeout.is_zero() {
        return future.await;
    }
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| anyhow::anyhow!("gRPC connection to {endpoint} timed out after {timeout:?}"))?
}

pub fn request<T>(endpoint: &NodeEndpoint, message: T) -> anyhow::Result<Request<T>> {
    Ok(with_deadline(
        request_with_context(endpoint, None, message)?,
        operon_core::runtime::TransportConfig::timeout(endpoint.transport.rpc_timeout_secs),
    ))
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

/// Apply an explicit call policy without changing authentication or context.
/// None is reserved for long-lived calls, not ordinary filesystem operations.
pub fn with_deadline<T>(mut request: Request<T>, timeout: Option<Duration>) -> Request<T> {
    if let Some(timeout) = timeout {
        request.set_timeout(timeout);
        request
            .extensions_mut()
            .insert(deadline_channel::RequestDeadline(timeout));
    }
    request
}

/// Bound response/header or chunk progress locally as well as advertising a
/// deadline to the peer. This does not replay a timed-out mutation.
pub async fn bounded_rpc<T>(
    timeout: Duration,
    future: impl Future<Output = Result<T, tonic::Status>>,
) -> Result<T, tonic::Status> {
    if timeout.is_zero() {
        return future.await;
    }
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| tonic::Status::deadline_exceeded("gRPC progress deadline exceeded"))?
}

/// Reset the inactivity deadline as the transport consumes request chunks.
/// Keep the final deadline armed after input EOF while awaiting acknowledgement.
pub async fn transfer_progress<T>(
    timeout: Duration,
    mut progress: tokio::sync::watch::Receiver<u64>,
    future: impl Future<Output = Result<T, tonic::Status>>,
) -> Result<T, tonic::Status> {
    if timeout.is_zero() {
        return future.await;
    }
    tokio::pin!(future);
    let timer = tokio::time::sleep(timeout);
    tokio::pin!(timer);
    let mut input_open = true;
    loop {
        tokio::select! {
            result = &mut future => return result,
            _ = &mut timer => return Err(tonic::Status::deadline_exceeded("gRPC upload progress deadline exceeded")),
            changed = progress.changed(), if input_open => {
                match changed {
                    Ok(()) => timer.as_mut().reset(tokio::time::Instant::now() + timeout),
                    Err(_) => input_open = false,
                }
            }
        }
    }
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
) -> impl Iterator<Item = WriteFileRequest> + Send + 'static {
    let expected_version_for_precondition = expected_version.clone();
    let target = WriteFileRequest {
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
    };
    std::iter::once(target).chain(byte_chunks(body).map(|chunk| WriteFileRequest {
        payload: Some(write_file_request::Payload::Chunk(chunk)),
    }))
}

pub fn chunk_stdin_requests(
    exec_id: String,
    body: &[u8],
) -> impl Iterator<Item = ExecStdinRequest> + Send + 'static {
    let target = ExecStdinRequest {
        payload: Some(exec_stdin_request::Payload::Target(ExecStdinTarget {
            exec_id,
        })),
    };
    std::iter::once(target).chain(byte_chunks(body).map(|chunk| ExecStdinRequest {
        payload: Some(exec_stdin_request::Payload::Chunk(chunk)),
    }))
}

fn byte_chunks(body: &[u8]) -> impl Iterator<Item = FileChunk> + Send + 'static {
    // A slice-based public helper must own input for tonic's 'static stream.
    // Only this source buffer and the currently polled chunk are retained;
    // file-based callers stream directly without this whole-input buffer.
    // The tonic request stream must own its input for its 'static lifetime.
    let owned = body.to_vec();
    let mut bytes = owned.into_iter();
    let mut empty_chunk = bytes.len() == 0;
    std::iter::from_fn(move || {
        if bytes.len() == 0 && !empty_chunk {
            return None;
        }
        empty_chunk = false;
        Some(FileChunk {
            data: bytes.by_ref().take(STREAM_CHUNK_BYTES).collect(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn progress_deadlines_expire_and_zero_disables_them() {
        let error = bounded_rpc(
            Duration::from_millis(10),
            std::future::pending::<Result<(), tonic::Status>>(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), tonic::Code::DeadlineExceeded);
        let result = bounded_rpc(Duration::ZERO, async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, tonic::Status>(42)
        })
        .await
        .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn upload_progress_resets_and_eof_still_bounds_acknowledgement() {
        let (sender, receiver) = tokio::sync::watch::channel(0_u64);
        let timeout = Duration::from_millis(40);
        let result = transfer_progress(timeout, receiver, async {
            for _ in 0..4 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                sender.send_modify(|count| *count += 1);
            }
            Ok::<_, tonic::Status>(42)
        })
        .await
        .unwrap();
        assert_eq!(result, 42);
        let (sender, receiver) = tokio::sync::watch::channel(0_u64);
        drop(sender);
        let error = transfer_progress(
            Duration::from_millis(10),
            receiver,
            std::future::pending::<Result<(), tonic::Status>>(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), tonic::Code::DeadlineExceeded);
    }

    #[test]
    fn deadline_policy_preserves_metadata_and_long_lived_requests() {
        let mut endpoint = NodeEndpoint {
            node_id: "remote".into(),
            endpoint: "grpc://example:7789".into(),
            token: Some("secret".into()),
            transport: Default::default(),
        };
        endpoint.transport.rpc_timeout_secs = 300;
        let request = request(&endpoint, ()).unwrap();
        assert!(request.metadata().contains_key("grpc-timeout"));
        assert_eq!(
            request.metadata().get("authorization").unwrap(),
            "Bearer secret"
        );
        let long_lived = request_with_context(&endpoint, None, ()).unwrap();
        assert!(!long_lived.metadata().contains_key("grpc-timeout"));
        endpoint.transport.rpc_timeout_secs = 0;
        assert!(!super::request(&endpoint, ())
            .unwrap()
            .metadata()
            .contains_key("grpc-timeout"));
    }
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
            transport: Default::default(),
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
            chunk_write_requests("/empty".to_string(), &[], None).count(),
            2
        );
        assert_eq!(chunk_stdin_requests("exec-1".to_string(), &[]).count(), 2);
    }

    #[test]
    fn chunks_write_target_can_include_expected_version() {
        let chunks = chunk_write_requests(
            "/file.txt".to_string(),
            &[1, 2, 3],
            Some("version-1".to_string()),
        )
        .collect::<Vec<_>>();
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
            transport: Default::default(),
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
            transport: Default::default(),
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

        let chunks = chunk_stdin_requests("exec-1".to_string(), &body).collect::<Vec<_>>();

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

        let chunks = chunk_write_requests("/file.bin".to_string(), &body, None).collect::<Vec<_>>();

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
