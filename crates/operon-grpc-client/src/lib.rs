use operon_core::RequestContext;
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    job_stdin_request, operon_runtime_client::OperonRuntimeClient, write_file_request, FileChunk,
    JobStdinRequest, JobStdinTarget, WriteFileRequest, WriteFileTarget,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};

pub const RUN_ID_METADATA: &str = "x-operon-run-id";
pub const STEP_ID_METADATA: &str = "x-operon-step-id";
pub const STREAM_CHUNK_BYTES: usize = 64 * 1024;

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
    let channel = Channel::from_shared(grpc_channel_uri(&endpoint.endpoint)?)?
        .connect()
        .await?;
    Ok(OperonRuntimeClient::new(channel))
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

pub fn chunk_write_requests(path: String, body: &[u8]) -> Vec<WriteFileRequest> {
    let mut chunks = vec![WriteFileRequest {
        payload: Some(write_file_request::Payload::Target(WriteFileTarget {
            path,
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

pub fn chunk_stdin_requests(job_id: String, body: &[u8]) -> Vec<JobStdinRequest> {
    let mut chunks = vec![JobStdinRequest {
        payload: Some(job_stdin_request::Payload::Target(JobStdinTarget {
            job_id,
        })),
    }];
    if body.is_empty() {
        chunks.push(JobStdinRequest {
            payload: Some(job_stdin_request::Payload::Chunk(FileChunk {
                data: Vec::new(),
            })),
        });
        return chunks;
    }
    chunks.extend(
        body.chunks(STREAM_CHUNK_BYTES)
            .map(|chunk| JobStdinRequest {
                payload: Some(job_stdin_request::Payload::Chunk(FileChunk {
                    data: chunk.to_vec(),
                })),
            }),
    );
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(chunk_write_requests("/empty".to_string(), &[]).len(), 2);
        assert_eq!(chunk_stdin_requests("job-1".to_string(), &[]).len(), 2);
    }
}
