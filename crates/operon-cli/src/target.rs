use std::path::PathBuf;

use operon_config::OperonConfig;

tokio::task_local! {
    pub(crate) static TRANSPORT_OVERRIDES: crate::cli_args::TransportOverrides;
}

pub(crate) fn endpoint_with_overrides(
    mut endpoint: operon_core::runtime::NodeEndpoint,
) -> operon_core::runtime::NodeEndpoint {
    let _ = TRANSPORT_OVERRIDES.try_with(|overrides| {
        for (value, target) in [
            (
                overrides.connect_timeout_secs,
                &mut endpoint.transport.connect_timeout_secs,
            ),
            (
                overrides.rpc_timeout_secs,
                &mut endpoint.transport.rpc_timeout_secs,
            ),
            (
                overrides.transfer_timeout_secs,
                &mut endpoint.transport.transfer_timeout_secs,
            ),
            (
                overrides.progress_timeout_secs,
                &mut endpoint.transport.progress_timeout_secs,
            ),
        ] {
            if let Some(value) = value {
                *target = value;
            }
        }
    });
    endpoint
}

#[derive(Debug)]
pub(crate) struct NodePath {
    pub(crate) node_id: String,
    pub(crate) path: String,
}

pub(crate) fn parse_node_path(target: &str) -> anyhow::Result<NodePath> {
    let (node_id, path) = target
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("target must be in node:/path form"))?;
    if node_id.is_empty() || path.is_empty() {
        anyhow::bail!("target must include node and path");
    }
    Ok(NodePath {
        node_id: node_id.to_string(),
        path: path.to_string(),
    })
}

pub(crate) fn load_endpoint(
    config_path: PathBuf,
    node_id: &str,
) -> anyhow::Result<operon_core::runtime::NodeEndpoint> {
    let config = OperonConfig::load(&config_path)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    let endpoint = endpoint_with_overrides(config.endpoint(node_id, &config_dir)?);
    endpoint.transport.validate().map_err(anyhow::Error::msg)?;
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transport_cli_overrides_are_scoped_and_preserve_other_settings() {
        let endpoint = operon_core::runtime::NodeEndpoint {
            node_id: "remote".into(),
            endpoint: "grpc://example:7789".into(),
            token: None,
            transport: Default::default(),
        };
        let overrides = crate::cli_args::TransportOverrides {
            rpc_timeout_secs: Some(300),
            progress_timeout_secs: Some(0),
            ..Default::default()
        };
        TRANSPORT_OVERRIDES
            .scope(overrides, async {
                let effective = endpoint_with_overrides(endpoint.clone());
                assert_eq!(effective.transport.rpc_timeout_secs, 300);
                assert_eq!(effective.transport.progress_timeout_secs, 0);
                assert_eq!(effective.transport.keepalive_timeout_secs, 10);
            })
            .await;
        assert_eq!(
            endpoint_with_overrides(endpoint).transport.rpc_timeout_secs,
            30
        );
    }

    #[test]
    fn parses_node_path_target() {
        let target = parse_node_path("node-a:/workspace/file.txt").expect("target should parse");

        assert_eq!(target.node_id, "node-a");
        assert_eq!(target.path, "/workspace/file.txt");
    }

    #[test]
    fn rejects_node_path_without_separator() {
        let error = parse_node_path("node-a/workspace").expect_err("target should fail");
        assert!(error.to_string().contains("node:/path"));
    }
}
