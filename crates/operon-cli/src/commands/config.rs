use std::path::Path;

use operon_config::{resolve_path, AuthConfig, OperonConfig};

use crate::commands::service::format_service_protocol;

mod explain;

pub(crate) use explain::explain;

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
    auth: String,
}

#[derive(Debug, serde::Serialize)]
struct PolicyExplain {
    subject: String,
    fs_mounts: Vec<FsMountExplain>,
    exec: ExecPolicyExplain,
    services: Vec<ServiceExplain>,
    effective_grants: Vec<PolicyGrantExplain>,
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
struct ExecPolicyExplain {
    allowed_cwds: Vec<String>,
    default_timeout_secs: u64,
    max_timeout_secs: u64,
    allow_sessions: bool,
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
struct PolicyGrantExplain {
    capability_id: String,
    action: String,
    resource: String,
    allowed: bool,
    reason_code: String,
}

#[derive(Debug, serde::Serialize)]
struct SecretsExplain {
    file: Option<String>,
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
                auth: auth_source(&node.auth, &config_dir),
            })
            .collect();
        let policy = config.policy.as_ref().map(|policy| {
            let effective_grants = effective_policy_grants(policy);
            PolicyExplain {
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
                exec: ExecPolicyExplain {
                    allowed_cwds: policy.exec.allowed_cwds.clone(),
                    default_timeout_secs: policy.exec.default_timeout_secs,
                    max_timeout_secs: policy.exec.max_timeout_secs,
                    allow_sessions: policy.exec.allow_sessions,
                    preserve_env: policy.exec.preserve_env,
                    env_allowlist: policy.exec.env_allowlist.clone(),
                    allowed_secrets: policy.exec.allowed_secrets.clone(),
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
                effective_grants,
            }
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

fn effective_policy_grants(policy: &operon_core::PolicyConfig) -> Vec<PolicyGrantExplain> {
    let mut grants = Vec::new();
    for mount in &policy.fs.mounts {
        grants.push(policy_grant(
            format!("fs:{}", mount.name),
            "read",
            &mount.path,
            mount.permissions.read,
            if mount.permissions.read {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::FsPermissionDenied
            },
        ));
        grants.push(policy_grant(
            format!("fs:{}", mount.name),
            "write",
            &mount.path,
            mount.permissions.write,
            if mount.permissions.write {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::FsPermissionDenied
            },
        ));
        grants.push(policy_grant(
            format!("fs:{}", mount.name),
            "delete",
            &mount.path,
            mount.permissions.delete,
            if mount.permissions.delete {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::FsPermissionDenied
            },
        ));
    }
    for cwd in &policy.exec.allowed_cwds {
        grants.push(policy_grant(
            "exec:default",
            "run",
            cwd,
            true,
            operon_core::PolicyReasonCode::Allowed,
        ));
        grants.push(policy_grant(
            "exec:default",
            "session",
            cwd,
            policy.exec.allow_sessions,
            if policy.exec.allow_sessions {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::ExecSessionDenied
            },
        ));
    }
    for secret in &policy.exec.allowed_secrets {
        grants.push(policy_grant(
            "secret:default",
            "use",
            secret,
            true,
            operon_core::PolicyReasonCode::Allowed,
        ));
    }
    for service in &policy.service.services {
        grants.push(policy_grant(
            format!("service:{}", service.id),
            "check",
            &service.id,
            service.permissions.check,
            if service.permissions.check {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::ServiceActionDenied
            },
        ));
        grants.push(policy_grant(
            format!("service:{}", service.id),
            "forward",
            &service.id,
            service.permissions.forward,
            if service.permissions.forward {
                operon_core::PolicyReasonCode::Allowed
            } else {
                operon_core::PolicyReasonCode::ServiceActionDenied
            },
        ));
    }
    grants
}

fn policy_grant(
    capability_id: impl Into<String>,
    action: impl Into<String>,
    resource: impl Into<String>,
    allowed: bool,
    reason_code: operon_core::PolicyReasonCode,
) -> PolicyGrantExplain {
    PolicyGrantExplain {
        capability_id: capability_id.into(),
        action: action.into(),
        resource: resource.into(),
        allowed,
        reason_code: reason_code.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init_config;
    use crate::output::OutputMode;

    #[test]
    fn config_explain_summarizes_unified_config_without_secret_values() {
        let base = tempfile::tempdir().expect("temp dir");
        let config_path = base.path().join("config.yaml");

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
        assert_eq!(explain.config_dir, base.path().display().to_string());
        let daemon = explain.daemon.expect("daemon explain");
        assert_eq!(daemon.node_id, "local");
        assert_eq!(
            daemon.auth,
            format!("token_file:{}", base.path().join("token").display())
        );
        assert!(!daemon.auth.contains("token: "));
        assert_eq!(explain.client.nodes.len(), 1);
        let policy = explain.policy.expect("policy explain");
        assert_eq!(policy.subject, "local-cli");
        assert_eq!(policy.fs_mounts[0].name, "workspace");
        assert_eq!(policy.services[0].protocol, "tcp");
        assert!(policy.services[0].check);
        assert!(policy.services[0].forward);
        assert!(policy.effective_grants.iter().any(|grant| {
            grant.capability_id == "fs:workspace"
                && grant.action == "read"
                && grant.resource == "/"
                && grant.allowed
                && grant.reason_code == "allowed"
        }));
        assert!(policy.effective_grants.iter().any(|grant| {
            grant.capability_id == "fs:workspace"
                && grant.action == "delete"
                && grant.resource == "/"
                && !grant.allowed
                && grant.reason_code == "fs-permission-denied"
        }));
        assert!(policy.effective_grants.iter().any(|grant| {
            grant.capability_id == "exec:default"
                && grant.action == "run"
                && grant.resource == "/"
                && grant.allowed
        }));
        assert!(policy.effective_grants.iter().any(|grant| {
            grant.capability_id == "service:local-daemon"
                && grant.action == "forward"
                && grant.resource == "local-daemon"
                && grant.allowed
        }));
        let expected_secrets = base.path().join("secrets.yaml").display().to_string();
        assert_eq!(
            explain.secrets.expect("secrets").file.as_deref(),
            Some(expected_secrets.as_str())
        );
    }
}
