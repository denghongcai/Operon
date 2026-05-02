use std::path::{Path, PathBuf};

use operon_config::{resolve_path, AuthConfig, OperonConfig};

use crate::{
    commands::service::format_service_protocol,
    output::{print_json, OutputMode},
};

#[derive(Debug, serde::Serialize)]
struct ConfigExplain {
    path: String,
    default_path: bool,
    config_dir: String,
    daemon: Option<DaemonExplain>,
    client: ClientExplain,
    policy: Option<PolicyExplain>,
    secrets: Option<SecretsExplain>,
}

#[derive(Debug, serde::Serialize)]
struct DaemonExplain {
    node_id: String,
    grpc_listen: String,
    workspace: String,
    advertise_lan: bool,
    store: Option<String>,
    auth: String,
}

#[derive(Debug, serde::Serialize)]
struct ClientExplain {
    nodes: Vec<NodeExplain>,
}

#[derive(Debug, serde::Serialize)]
struct NodeExplain {
    node_id: String,
    endpoint: String,
    provider: String,
    auth: String,
}

#[derive(Debug, serde::Serialize)]
struct PolicyExplain {
    subject: String,
    fs_mounts: Vec<FsMountExplain>,
    job: JobPolicyExplain,
    services: Vec<ServiceExplain>,
}

#[derive(Debug, serde::Serialize)]
struct FsMountExplain {
    name: String,
    path: String,
    read: bool,
    write: bool,
    delete: bool,
}

#[derive(Debug, serde::Serialize)]
struct JobPolicyExplain {
    allowed_cwds: Vec<String>,
    default_timeout_secs: u64,
    max_timeout_secs: u64,
    preserve_env: bool,
    env_allowlist: Vec<String>,
    allowed_secrets: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct ServiceExplain {
    id: String,
    name: String,
    endpoint: String,
    protocol: String,
    description: String,
    check: bool,
    forward: bool,
}

#[derive(Debug, serde::Serialize)]
struct SecretsExplain {
    file: Option<String>,
}

pub(crate) fn explain(config_path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let config = OperonConfig::load(&config_path)?;
    let explain = ConfigExplain::from_config(&config_path, &config);
    if output.json {
        print_json(&explain)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_config_explain(&explain);
    Ok(())
}

impl ConfigExplain {
    fn from_config(config_path: &Path, config: &OperonConfig) -> Self {
        let config_dir = OperonConfig::config_dir(config_path);
        let default_path = OperonConfig::default_path();
        let daemon = config.daemon.as_ref().map(|daemon| DaemonExplain {
            node_id: daemon.node_id.clone(),
            grpc_listen: daemon.grpc_listen.to_string(),
            workspace: display_path(resolve_path(&config_dir, &daemon.workspace)),
            advertise_lan: daemon.advertise_lan,
            store: daemon
                .store
                .as_ref()
                .map(|path| display_path(resolve_path(&config_dir, path))),
            auth: auth_source(&daemon.auth, &config_dir),
        });
        let nodes = config
            .client
            .nodes
            .iter()
            .map(|(node_id, node)| NodeExplain {
                node_id: node_id.clone(),
                endpoint: node.endpoint.clone(),
                provider: node.provider.as_str().to_string(),
                auth: auth_source(&node.auth, &config_dir),
            })
            .collect();
        let policy = config.policy.as_ref().map(|policy| PolicyExplain {
            subject: policy.subject.clone(),
            fs_mounts: policy
                .fs
                .mounts
                .iter()
                .map(|mount| FsMountExplain {
                    name: mount.name.clone(),
                    path: mount.path.clone(),
                    read: mount.permissions.read,
                    write: mount.permissions.write,
                    delete: mount.permissions.delete,
                })
                .collect(),
            job: JobPolicyExplain {
                allowed_cwds: policy.job.allowed_cwds.clone(),
                default_timeout_secs: policy.job.default_timeout_secs,
                max_timeout_secs: policy.job.max_timeout_secs,
                preserve_env: policy.job.preserve_env,
                env_allowlist: policy.job.env_allowlist.clone(),
                allowed_secrets: policy.job.allowed_secrets.clone(),
            },
            services: policy
                .service
                .services
                .iter()
                .map(|service| ServiceExplain {
                    id: service.id.clone(),
                    name: service.name.clone(),
                    endpoint: format!("{}:{}", service.host, service.port),
                    protocol: format_service_protocol(&service.protocol).to_string(),
                    description: service.description.clone(),
                    check: service.permissions.check,
                    forward: service.permissions.forward,
                })
                .collect(),
        });
        let secrets = config.secrets.as_ref().map(|secrets| SecretsExplain {
            file: secrets
                .file
                .as_ref()
                .map(|path| display_path(resolve_path(&config_dir, path))),
        });

        Self {
            path: display_path(config_path),
            default_path: config_path == default_path,
            config_dir: display_path(&config_dir),
            daemon,
            client: ClientExplain { nodes },
            policy,
            secrets,
        }
    }
}

fn print_config_explain(explain: &ConfigExplain) {
    println!("config: {}", explain.path);
    println!("default_path: {}", explain.default_path);
    println!("config_dir: {}", explain.config_dir);

    match &explain.daemon {
        Some(daemon) => {
            println!("daemon:");
            println!("  node_id: {}", daemon.node_id);
            println!("  grpc_listen: {}", daemon.grpc_listen);
            println!("  workspace: {}", daemon.workspace);
            println!("  advertise_lan: {}", daemon.advertise_lan);
            println!("  store: {}", daemon.store.as_deref().unwrap_or("<none>"));
            println!("  auth: {}", daemon.auth);
        }
        None => println!("daemon: <none>"),
    }

    println!("client nodes:");
    if explain.client.nodes.is_empty() {
        println!("  <none>");
    }
    for node in &explain.client.nodes {
        println!(
            "  {} -> {} ({}, auth: {})",
            node.node_id, node.endpoint, node.provider, node.auth
        );
    }

    match &explain.policy {
        Some(policy) => {
            println!("policy:");
            println!("  subject: {}", policy.subject);
            println!("  fs mounts:");
            if policy.fs_mounts.is_empty() {
                println!("    <none>");
            }
            for mount in &policy.fs_mounts {
                println!(
                    "    {} path={} read={} write={} delete={}",
                    mount.name, mount.path, mount.read, mount.write, mount.delete
                );
            }
            println!(
                "  job: allowed_cwds={} default_timeout={} max_timeout={} preserve_env={} env_allowlist={} allowed_secrets={}",
                policy.job.allowed_cwds.join(","),
                policy.job.default_timeout_secs,
                policy.job.max_timeout_secs,
                policy.job.preserve_env,
                policy.job.env_allowlist.join(","),
                policy.job.allowed_secrets.join(",")
            );
            println!("  services:");
            if policy.services.is_empty() {
                println!("    <none>");
            }
            for service in &policy.services {
                println!(
                    "    {} {} {} {} check={} forward={} - {}",
                    service.id,
                    service.protocol,
                    service.endpoint,
                    service.name,
                    service.check,
                    service.forward,
                    service.description
                );
            }
        }
        None => println!("policy: <none>"),
    }

    match &explain.secrets {
        Some(secrets) => println!("secrets: {}", secrets.file.as_deref().unwrap_or("<none>")),
        None => println!("secrets: <none>"),
    }
}

fn auth_source(auth: &AuthConfig, config_dir: &Path) -> String {
    match (&auth.token, &auth.token_file, &auth.token_env) {
        (None, None, None) => "none".to_string(),
        (Some(_), None, None) => "inline".to_string(),
        (None, Some(path), None) => {
            format!(
                "token_file:{}",
                display_path(resolve_path(config_dir, path))
            )
        }
        (None, None, Some(name)) => format!("token_env:{name}"),
        _ => "invalid:multiple-sources".to_string(),
    }
}

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init_config;
    use crate::output::OutputMode;

    #[test]
    fn config_explain_summarizes_unified_config_without_secret_values() {
        let base = unique_temp_dir("operon-config-explain-test");
        let config_path = base.join("config.yaml");

        init_config(
            config_path.clone(),
            OutputMode {
                json: false,
                quiet: true,
            },
        )
        .expect("init config");
        let config = OperonConfig::load(&config_path).expect("config should load");
        let explain = ConfigExplain::from_config(&config_path, &config);

        assert_eq!(explain.path, config_path.display().to_string());
        assert_eq!(explain.config_dir, base.display().to_string());
        let daemon = explain.daemon.expect("daemon explain");
        assert_eq!(daemon.node_id, "local");
        assert_eq!(
            daemon.auth,
            format!("token_file:{}", base.join("token").display())
        );
        assert!(!daemon.auth.contains("token: "));
        assert_eq!(explain.client.nodes.len(), 1);
        assert_eq!(explain.client.nodes[0].provider, "manual");
        let policy = explain.policy.expect("policy explain");
        assert_eq!(policy.subject, "local-cli");
        assert_eq!(policy.fs_mounts[0].name, "workspace");
        assert_eq!(policy.services[0].protocol, "tcp");
        assert!(policy.services[0].check);
        assert!(policy.services[0].forward);
        let expected_secrets = base.join("secrets.yaml").display().to_string();
        assert_eq!(
            explain.secrets.expect("secrets").file.as_deref(),
            Some(expected_secrets.as_str())
        );
        let _ = std::fs::remove_dir_all(base);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }
}
