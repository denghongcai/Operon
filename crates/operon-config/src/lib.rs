use std::{
    collections::BTreeMap,
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use operon_core::PolicyConfig;

mod warnings;

use warnings::collect_unknown_config_fields;
pub use warnings::ConfigWarning;

#[derive(Debug, Clone)]
pub struct LoadedOperonConfig {
    pub config: OperonConfig,
    pub warnings: Vec<ConfigWarning>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperonConfig {
    pub version: u32,
    #[serde(default)]
    pub daemon: Option<DaemonConfig>,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub policy: Option<PolicyConfig>,
    #[serde(default)]
    pub secrets: Option<SecretsConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonConfig {
    pub node_id: String,
    pub grpc_listen: SocketAddr,
    pub workspace: PathBuf,
    #[serde(default)]
    pub advertise_lan: bool,
    #[serde(default)]
    pub store: Option<PathBuf>,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ClientConfig {
    #[serde(default)]
    pub nodes: BTreeMap<String, NodeConfig>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SecretsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub node_id: String,
    pub endpoint: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeConfig {
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "AuthConfig::is_empty")]
    pub auth: AuthConfig,
}

impl OperonConfig {
    pub fn default_path() -> PathBuf {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".operon")
            .join("config.yaml")
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let loaded = Self::from_str_with_warnings(&content)?;
        for warning in &loaded.warnings {
            eprintln!("warning: unknown config field `{}` ignored", warning.path);
        }
        Ok(loaded.config)
    }

    pub fn from_str_with_warnings(content: &str) -> anyhow::Result<LoadedOperonConfig> {
        let value: serde_yaml::Value = serde_yaml::from_str(content)?;
        let warnings = collect_unknown_config_fields(&value);
        let config: Self = serde_yaml::from_value(value)?;
        if config.version != 1 {
            anyhow::bail!("unsupported config version `{}`", config.version);
        }
        Ok(LoadedOperonConfig { config, warnings })
    }

    pub fn config_dir(path: impl AsRef<Path>) -> PathBuf {
        path.as_ref()
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    pub fn endpoints(&self, config_dir: &Path) -> anyhow::Result<Vec<NodeEndpoint>> {
        self.client
            .nodes
            .iter()
            .map(|(node_id, node)| node.to_endpoint(node_id, config_dir))
            .collect()
    }

    pub fn endpoint(&self, node_id: &str, config_dir: &Path) -> anyhow::Result<NodeEndpoint> {
        let node = self
            .client
            .nodes
            .get(node_id)
            .ok_or_else(|| anyhow::anyhow!("node `{node_id}` not found in config"))?;
        node.to_endpoint(node_id, config_dir)
    }
}

impl NodeConfig {
    pub fn to_endpoint(&self, node_id: &str, config_dir: &Path) -> anyhow::Result<NodeEndpoint> {
        Ok(NodeEndpoint {
            node_id: node_id.to_string(),
            endpoint: self.endpoint.clone(),
            token: self.auth.resolve(config_dir)?,
        })
    }
}

impl AuthConfig {
    pub fn is_empty(&self) -> bool {
        self.token.is_none() && self.token_file.is_none() && self.token_env.is_none()
    }

    pub fn resolve(&self, config_dir: &Path) -> anyhow::Result<Option<String>> {
        let mut values = Vec::new();
        if let Some(token) = &self.token {
            values.push(token.clone());
        }
        if let Some(path) = &self.token_file {
            let path = resolve_path(config_dir, path);
            values.push(fs::read_to_string(path)?.trim().to_string());
        }
        if let Some(name) = &self.token_env {
            values.push(env::var(name)?);
        }
        match values.len() {
            0 => Ok(None),
            1 => Ok(values.into_iter().next()),
            _ => anyhow::bail!("auth must use only one of token, token_file, or token_env"),
        }
    }
}

pub fn resolve_path(config_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unknown_fields_without_blocking_config_parse() {
        let loaded = OperonConfig::from_str_with_warnings(
            r#"
version: 1
unexpected_root: true
daemon:
  node_id: local
  grpc_listen: 127.0.0.1:7789
  workspace: /workspace
  extra_daemon: true
client:
  nodes:
    gpu:
      endpoint: grpc://100.96.18.20:7789
      provider: tailscale
      auth:
        token: test-token
        ignored_auth: true
      extra_node: true
secrets:
  file: secrets.yaml
  extra_secrets: true
"#,
        )
        .expect("config should parse despite unknown fields");

        let mut warning_paths: Vec<_> = loaded
            .warnings
            .iter()
            .map(|warning| warning.path.as_str())
            .collect();
        warning_paths.sort_unstable();
        assert_eq!(
            warning_paths,
            vec![
                "client.nodes.gpu.auth.ignored_auth",
                "client.nodes.gpu.extra_node",
                "client.nodes.gpu.provider",
                "daemon.extra_daemon",
                "secrets.extra_secrets",
                "unexpected_root",
            ]
        );
        let endpoint = loaded
            .config
            .endpoint("gpu", Path::new("."))
            .expect("gpu endpoint");
        assert_eq!(endpoint.endpoint, "grpc://100.96.18.20:7789");
    }

    #[test]
    fn loads_unified_config_with_client_nodes() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
"#,
        )
        .expect("config should parse");

        let endpoint = config
            .endpoint("local", Path::new("."))
            .expect("local endpoint");
        assert_eq!(endpoint.node_id, "local");
        assert_eq!(endpoint.endpoint, "grpc://127.0.0.1:7789");
        assert_eq!(endpoint.token, None);
    }

    #[test]
    fn ignores_legacy_provider_field_in_client_node_config() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    gpu:
      endpoint: grpc://100.96.18.20:7789
      provider: tailscale
"#,
        )
        .expect("provider should not affect endpoint config");

        let endpoint = config
            .endpoint("gpu", Path::new("."))
            .expect("gpu endpoint");
        assert_eq!(endpoint.node_id, "gpu");
        assert_eq!(endpoint.endpoint, "grpc://100.96.18.20:7789");
    }

    #[test]
    fn returns_endpoints_in_node_id_order() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    node-b:
      endpoint: grpc://127.0.0.1:17791
    node-a:
      endpoint: grpc://127.0.0.1:17790
"#,
        )
        .expect("config should parse");

        let ids: Vec<_> = config
            .endpoints(Path::new("."))
            .expect("endpoints")
            .into_iter()
            .map(|endpoint| endpoint.node_id)
            .collect();

        assert_eq!(ids, vec!["node-a", "node-b"]);
    }

    #[test]
    fn resolves_inline_node_token() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      auth:
        token: test-token
"#,
        )
        .expect("config should parse");

        let endpoint = config
            .endpoint("local", Path::new("."))
            .expect("local endpoint");
        assert_eq!(endpoint.token.as_deref(), Some("test-token"));
    }

    #[test]
    fn omits_empty_auth_when_serializing_node() {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "local".to_string(),
            NodeConfig {
                endpoint: "grpc://127.0.0.1:7789".to_string(),
                auth: AuthConfig::default(),
            },
        );

        let yaml = serde_yaml::to_string(&OperonConfig {
            version: 1,
            daemon: None,
            client: ClientConfig { nodes },
            policy: None,
            secrets: None,
        })
        .expect("config should serialize");

        assert!(!yaml.contains("auth:"));
    }
}
