use operon_core::{
    CapabilityDiagnosticRequest, CapabilityList, HealthStatus, NodeInfo, PolicyDecision,
    RequestContext,
};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    operon_runtime_client::OperonRuntimeClient, GetNodeRequest, HealthRequest,
    ListCapabilitiesRequest,
};
use tonic::transport::Channel;

pub use crate::grpc_audit::list_audit;
pub use crate::grpc_exec_api::{
    cancel_exec, close_exec_stdin, get_exec, list_exec_logs, list_execs, run_exec,
    stream_exec_logs, watch_exec_to_terminal, write_exec_stdin_bytes, write_exec_stdin_file,
};
pub use crate::grpc_fs::{
    fs_copy, fs_delete, fs_list, fs_mkdir, fs_rename, fs_stat, fs_truncate, read_file_to_writer,
    write_file, write_file_bytes,
};
pub use crate::grpc_service_api::{check_service, list_services};

pub(crate) const DEFAULT_LIST_PAGE_SIZE: u32 = 1000;

tokio::task_local! {
    static REQUEST_CONTEXT: RequestContext;
}

pub async fn with_request_context<T, Fut>(
    context: RequestContext,
    f: impl FnOnce() -> Fut,
) -> anyhow::Result<T>
where
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    REQUEST_CONTEXT.scope(context, f()).await
}

pub async fn health_and_node(endpoint: &NodeEndpoint) -> anyhow::Result<(HealthStatus, NodeInfo)> {
    call(endpoint, |mut client, endpoint| async move {
        let health = client
            .health(with_auth(&endpoint, HealthRequest {})?)
            .await?
            .into_inner()
            .into();
        let node = client
            .get_node(with_auth(&endpoint, GetNodeRequest {})?)
            .await?
            .into_inner()
            .into();
        Ok((health, node))
    })
    .await
}

pub async fn list_capabilities(endpoint: &NodeEndpoint) -> anyhow::Result<CapabilityList> {
    let mut capabilities = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_capabilities(with_auth(
                        &endpoint,
                        ListCapabilitiesRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        capabilities.extend(
            response
                .capabilities
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(anyhow::Error::msg)?,
        );
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(CapabilityList {
        capabilities,
        next_page_token: String::new(),
    })
}

pub async fn explain_capability(
    endpoint: &NodeEndpoint,
    request: CapabilityDiagnosticRequest,
) -> anyhow::Result<PolicyDecision> {
    call(endpoint, |mut client, endpoint| async move {
        client
            .explain_capability(with_auth(&endpoint, request.into())?)
            .await?
            .into_inner()
            .try_into()
            .map_err(anyhow::Error::msg)
    })
    .await
}

pub(crate) async fn call<T, Fut>(
    endpoint: &NodeEndpoint,
    f: impl FnOnce(OperonRuntimeClient<Channel>, NodeEndpoint) -> Fut,
) -> anyhow::Result<T>
where
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let endpoint = endpoint.clone();
    let client = operon_grpc_client::connect(&endpoint).await?;
    f(client, endpoint).await
}

pub(crate) fn with_auth<T>(
    endpoint: &NodeEndpoint,
    message: T,
) -> anyhow::Result<tonic::Request<T>> {
    if let Ok(context) = REQUEST_CONTEXT.try_with(Clone::clone) {
        return operon_grpc_client::request_with_context(endpoint, Some(&context), message);
    }
    operon_grpc_client::request(endpoint, message)
}

pub(crate) fn grpc_exec_run_request(
    value: operon_core::ExecRunRequest,
) -> operon_protocol::runtime::v1::ExecRunRequest {
    operon_protocol::runtime::v1::ExecRunRequest {
        command: value.command,
        cwd: value.cwd.unwrap_or_default(),
        timeout_secs: value.timeout_secs,
        secrets: value.secrets,
        argv: value.argv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_grpc_uri_to_tonic_http_uri() {
        assert_eq!(
            operon_grpc_client::grpc_channel_uri("grpc://127.0.0.1:7789").expect("uri"),
            "http://127.0.0.1:7789"
        );
    }

    #[tokio::test]
    async fn with_auth_includes_execution_context_metadata() {
        let endpoint = NodeEndpoint {
            node_id: "node-a".to_string(),
            endpoint: "grpc://127.0.0.1:7789".to_string(),
            token: Some("test-token".to_string()),
        };

        with_request_context(
            RequestContext {
                run_id: Some("run-1".to_string()),
                step_id: Some("step-1".to_string()),
            },
            || async {
                let request = with_auth(&endpoint, HealthRequest {})?;
                assert_eq!(
                    request
                        .metadata()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer test-token")
                );
                assert_eq!(
                    request
                        .metadata()
                        .get("x-operon-run-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("run-1")
                );
                assert_eq!(
                    request
                        .metadata()
                        .get("x-operon-step-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("step-1")
                );
                Ok(())
            },
        )
        .await
        .expect("context metadata");
    }
}
