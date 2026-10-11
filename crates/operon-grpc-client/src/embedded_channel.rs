use anyhow::Context;
use operon_core::runtime::NodeEndpoint;
use operon_transport::embedded::{target_address, EmbeddedEndpoint};
use std::{path::Path, sync::Arc, time::Duration};
use tonic::transport::{Channel, Endpoint};

pub(crate) async fn connect(
    node: &NodeEndpoint,
    channel: Endpoint,
    timeout: Duration,
) -> anyhow::Result<Channel> {
    let config = node
        .transport
        .iroh
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("iroh:// endpoints require transport.iroh settings"))?;
    let target = target_address(&node.endpoint, config)?;
    let endpoint = Arc::new(EmbeddedEndpoint::from_config(&config.local, Path::new(".")).await?);
    if target.id == endpoint.id() {
        endpoint.close().await;
        anyhow::bail!("iroh client and server must use distinct private identity files");
    }
    if config.peer_addrs.is_empty() {
        if let Err(error) = endpoint
            .wait_online(Duration::from_secs(config.local.readiness_timeout_secs))
            .await
        {
            endpoint.close().await;
            return Err(error.into());
        }
    }
    let connector_endpoint = endpoint.clone();
    let connector = tower::service_fn(move |_| {
        let endpoint = connector_endpoint.clone();
        let target = target.clone();
        async move {
            endpoint
                .connect(target, timeout)
                .await
                .map(hyper_util::rt::TokioIo::new)
        }
    });
    let result = channel.connect_with_connector(connector).await;
    if result.is_err() {
        endpoint.close().await;
    }
    result.with_context(|| format!("failed to connect to iroh node {}", node.node_id))
}
