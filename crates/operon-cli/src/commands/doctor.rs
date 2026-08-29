use std::{path::PathBuf, time::Duration};

use operon_config::{resolve_path, validate_private_file_permissions, OperonConfig};
use operon_core::{CapabilityDiagnosticRequest, CapabilityKind, PolicyDecision, ServiceCheck};

use crate::{
    commands::mount_runtime,
    grpc,
    output::{print_json, OutputMode},
    private_files,
};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, serde::Serialize)]
pub(crate) struct DoctorReport {
    config_path: String,
    platform: DoctorPlatformReport,
    config_warnings: Vec<String>,
    security_diagnostics: Vec<DoctorSecurityDiagnostic>,
    nodes: Vec<DoctorNodeReport>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorSecurityDiagnostic {
    id: String,
    ok: bool,
    severity: String,
    message: String,
    hint: String,
}

#[derive(Debug, serde::Serialize)]
struct DoctorPlatformReport {
    os: String,
    arch: String,
    mount_adapter: String,
    mount_runtime: String,
    mount_runtime_ready: bool,
    mount_hint: String,
    private_file_protection: String,
    exec_cancellation: String,
    pty_sessions: String,
    service_forwarding: String,
}

#[derive(Debug, serde::Serialize)]
struct DoctorNodeReport {
    node_id: String,
    endpoint: Option<String>,
    endpoint_ok: bool,
    endpoint_error: Option<String>,
    health_ok: bool,
    health_error: Option<String>,
    runtime_node_id: Option<String>,
    runtime_version: Option<String>,
    protocol_version: String,
    protocol_match: Option<bool>,
    capability_diagnostics: Vec<PolicyDecision>,
    capability_error: Option<String>,
    service_checks: Vec<ServiceCheck>,
    service_error: Option<String>,
}

pub(crate) async fn run(
    config_path: PathBuf,
    nodes: Vec<String>,
    mount_runtime_only: bool,
    output: OutputMode,
) -> anyhow::Result<()> {
    if mount_runtime_only {
        let report = platform_report();
        if output.json {
            print_json(&report)?;
        } else if !output.quiet {
            print_platform_report(&report);
        }
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let loaded = OperonConfig::from_str_with_warnings(&content)?;
    let config_dir = OperonConfig::config_dir(&config_path);
    let node_ids = if nodes.is_empty() {
        loaded
            .config
            .client
            .nodes
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    } else {
        nodes
    };

    let mut node_reports = Vec::new();
    for node_id in node_ids {
        let endpoint = loaded.config.endpoint(&node_id, &config_dir);
        let report = match endpoint {
            Ok(endpoint) => diagnose_node(node_id, endpoint).await,
            Err(error) => DoctorNodeReport {
                node_id,
                endpoint: None,
                endpoint_ok: false,
                endpoint_error: Some(error.to_string()),
                health_ok: false,
                health_error: None,
                runtime_node_id: None,
                runtime_version: None,
                protocol_version: operon_protocol::PROTOCOL_VERSION.to_string(),
                protocol_match: None,
                capability_diagnostics: Vec::new(),
                capability_error: None,
                service_checks: Vec::new(),
                service_error: None,
            },
        };
        node_reports.push(report);
    }

    let report = DoctorReport {
        config_path: config_path.display().to_string(),
        platform: platform_report(),
        config_warnings: loaded
            .warnings
            .into_iter()
            .map(|warning| warning.path)
            .collect(),
        security_diagnostics: security_diagnostics(&loaded.config, &config_dir, &config_path),
        nodes: node_reports,
    };

    if output.json {
        print_json(&report)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    print_report(&report);
    Ok(())
}

fn platform_report() -> DoctorPlatformReport {
    let mount_runtime = mount_runtime::report();
    DoctorPlatformReport {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        mount_adapter: mount_runtime.adapter.to_string(),
        mount_runtime: mount_runtime.status.to_string(),
        mount_runtime_ready: mount_runtime.ready,
        mount_hint: mount_runtime.hint.to_string(),
        private_file_protection: private_file_protection_diagnostic().to_string(),
        exec_cancellation: exec_cancellation_diagnostic().to_string(),
        pty_sessions: pty_session_diagnostic().to_string(),
        service_forwarding: "service forwarding depends on local and remote firewall policy"
            .to_string(),
    }
}

fn security_diagnostics(
    config: &OperonConfig,
    config_dir: &std::path::Path,
    config_path: &std::path::Path,
) -> Vec<DoctorSecurityDiagnostic> {
    let mut diagnostics = vec![private_file_diagnostic(
        "config-file-private",
        config_path,
        "config.yaml should be private when it contains inline tokens or secret references",
    )];
    if let Some(daemon) = &config.daemon {
        let has_auth = !daemon.auth.is_empty();
        let non_loopback = !daemon.grpc_listen.ip().is_loopback();
        diagnostics.push(DoctorSecurityDiagnostic {
            id: "daemon-non-loopback-auth".to_string(),
            ok: has_auth || !non_loopback,
            severity: if has_auth || !non_loopback {
                "info"
            } else {
                "error"
            }
            .to_string(),
            message: if non_loopback {
                format!(
                    "daemon grpc_listen {} is reachable beyond loopback",
                    daemon.grpc_listen
                )
            } else {
                format!("daemon grpc_listen {} is loopback-only", daemon.grpc_listen)
            },
            hint: if non_loopback && !has_auth {
                "configure daemon.auth.token_file or bind grpc_listen to 127.0.0.1/::1".to_string()
            } else {
                "non-loopback daemon listeners require bearer-token auth".to_string()
            },
        });
        if let Some(path) = &daemon.auth.token_file {
            diagnostics.push(private_file_diagnostic(
                "daemon-token-file-private",
                &resolve_path(config_dir, path),
                "daemon auth token_file must be readable only by the owner",
            ));
        }
        diagnostics.push(DoctorSecurityDiagnostic {
            id: "daemon-auth-reference".to_string(),
            ok: daemon.auth.token.is_none(),
            severity: if daemon.auth.token.is_none() {
                "info"
            } else {
                "warning"
            }
            .to_string(),
            message: if daemon.auth.token.is_some() {
                "daemon auth uses an inline token".to_string()
            } else if daemon.auth.token_file.is_some() {
                "daemon auth uses a token_file reference".to_string()
            } else if daemon.auth.token_env.is_some() {
                "daemon auth uses a token_env reference".to_string()
            } else {
                "daemon auth is not configured".to_string()
            },
            hint: "prefer daemon.auth.token_file or token_env over inline daemon.auth.token"
                .to_string(),
        });
    } else {
        diagnostics.push(DoctorSecurityDiagnostic {
            id: "daemon-section".to_string(),
            ok: true,
            severity: "info".to_string(),
            message: "config has no daemon section; local daemon listener checks skipped"
                .to_string(),
            hint: "daemon listener security is checked when a daemon section is present"
                .to_string(),
        });
    }

    for (node_id, node) in &config.client.nodes {
        if let Some(path) = &node.auth.token_file {
            diagnostics.push(private_file_diagnostic(
                &format!("client-node-{node_id}-token-file-private"),
                &resolve_path(config_dir, path),
                "client auth token_file should be readable only by the owner",
            ));
        }
        diagnostics.push(DoctorSecurityDiagnostic {
            id: format!("client-node-{node_id}-auth-reference"),
            ok: node.auth.token.is_none(),
            severity: if node.auth.token.is_none() {
                "info"
            } else {
                "warning"
            }
            .to_string(),
            message: if node.auth.token.is_some() {
                format!("client node `{node_id}` uses an inline token")
            } else if node.auth.token_file.is_some() {
                format!("client node `{node_id}` uses a token_file reference")
            } else if node.auth.token_env.is_some() {
                format!("client node `{node_id}` uses a token_env reference")
            } else {
                format!("client node `{node_id}` has no bearer token configured")
            },
            hint: "prefer client auth.token_file or auth.token_env over inline auth.token"
                .to_string(),
        });
    }

    if let Some(path) = config
        .secrets
        .as_ref()
        .and_then(|secrets| secrets.file.as_ref())
    {
        diagnostics.push(private_file_diagnostic(
            "secrets-file-private",
            &resolve_path(config_dir, path),
            "secrets.file should be readable only by the owner",
        ));
    }

    diagnostics.extend(service_permission_diagnostics(config));
    diagnostics
}

fn private_file_diagnostic(
    id: &str,
    path: &std::path::Path,
    hint: &str,
) -> DoctorSecurityDiagnostic {
    match validate_private_file_permissions(path) {
        Ok(()) => DoctorSecurityDiagnostic {
            id: id.to_string(),
            ok: true,
            severity: "info".to_string(),
            message: format!("private file `{}` passed permission checks", path.display()),
            hint: hint.to_string(),
        },
        Err(error) => DoctorSecurityDiagnostic {
            id: id.to_string(),
            ok: false,
            severity: "warning".to_string(),
            message: format!(
                "private file `{}` failed permission checks: {error}",
                path.display()
            ),
            hint: hint.to_string(),
        },
    }
}

fn service_permission_diagnostics(config: &OperonConfig) -> Vec<DoctorSecurityDiagnostic> {
    let Some(policy) = &config.policy else {
        return Vec::new();
    };

    policy
        .service
        .services
        .iter()
        .map(|service| {
            let ok = service.permissions.check || service.permissions.forward;
            DoctorSecurityDiagnostic {
                id: format!("service-{}-permissions", service.id),
                ok,
                severity: if ok { "info" } else { "warning" }.to_string(),
                message: format!(
                    "service `{}` permissions check={} forward={}",
                    service.id, service.permissions.check, service.permissions.forward
                ),
                hint: "service permissions are default-deny; set check and/or forward explicitly when access is intended".to_string(),
            }
        })
        .collect()
}

fn private_file_protection_diagnostic() -> &'static str {
    match private_files::private_file_security_model() {
        "unix-owner-only-mode" => "unix-owner-only-mode-0600",
        "windows-acl-verified" => "windows-acl-verified",
        _ => "private-file-permission-warning",
    }
}

#[cfg(unix)]
fn exec_cancellation_diagnostic() -> &'static str {
    "process-group-termination"
}

#[cfg(windows)]
fn exec_cancellation_diagnostic() -> &'static str {
    "job-object-process-tree-termination"
}

#[cfg(all(not(unix), not(windows)))]
fn exec_cancellation_diagnostic() -> &'static str {
    "direct-child-best-effort"
}

#[cfg(not(windows))]
fn pty_session_diagnostic() -> &'static str {
    "portable-pty-smoke-validated"
}

#[cfg(windows)]
fn pty_session_diagnostic() -> &'static str {
    "windows-portable-pty-smoke-validated"
}

async fn diagnose_node(
    node_id: String,
    endpoint: operon_core::runtime::NodeEndpoint,
) -> DoctorNodeReport {
    let mut report = DoctorNodeReport {
        node_id,
        endpoint: Some(endpoint.endpoint.clone()),
        endpoint_ok: true,
        endpoint_error: None,
        health_ok: false,
        health_error: None,
        runtime_node_id: None,
        runtime_version: None,
        protocol_version: operon_protocol::PROTOCOL_VERSION.to_string(),
        protocol_match: None,
        capability_diagnostics: Vec::new(),
        capability_error: None,
        service_checks: Vec::new(),
        service_error: None,
    };

    match tokio::time::timeout(HEALTH_TIMEOUT, grpc::health_and_node(&endpoint)).await {
        Ok(Ok((health, node))) => {
            report.health_ok = health.ok;
            report.runtime_node_id = Some(node.id);
            report.protocol_match = Some(health.version == operon_protocol::PROTOCOL_VERSION);
            report.runtime_version = Some(health.version);
        }
        Ok(Err(error)) => report.health_error = Some(error.to_string()),
        Err(_) => report.health_error = Some("health check timed out".to_string()),
    }

    match grpc::list_capabilities(&endpoint).await {
        Ok(capabilities) => {
            for capability in capabilities.capabilities {
                if let Some(request) = diagnostic_request_for_capability(&capability) {
                    match grpc::explain_capability(&endpoint, request).await {
                        Ok(decision) => report.capability_diagnostics.push(decision),
                        Err(error) => {
                            report.capability_error = Some(error.to_string());
                            break;
                        }
                    }
                }
            }
        }
        Err(error) => report.capability_error = Some(error.to_string()),
    }

    match grpc::list_services(&endpoint).await {
        Ok(services) => {
            for service in services
                .services
                .into_iter()
                .filter(|service| service.permissions.check)
            {
                match grpc::check_service(&endpoint, &service.id).await {
                    Ok(check) => report.service_checks.push(check),
                    Err(error) => {
                        report.service_error = Some(error.to_string());
                        break;
                    }
                }
            }
        }
        Err(error) => report.service_error = Some(error.to_string()),
    }

    report
}

fn diagnostic_request_for_capability(
    capability: &operon_core::Capability,
) -> Option<CapabilityDiagnosticRequest> {
    match &capability.kind {
        CapabilityKind::Fs => Some(CapabilityDiagnosticRequest {
            capability_id: capability.id.clone(),
            action: "read".to_string(),
            resource: "/".to_string(),
            timeout_secs: None,
        }),
        CapabilityKind::Exec => Some(CapabilityDiagnosticRequest {
            capability_id: capability.id.clone(),
            action: "run".to_string(),
            resource: "/".to_string(),
            timeout_secs: Some(1),
        }),
        CapabilityKind::Service => {
            let service_id = capability
                .id
                .strip_prefix("service:")
                .unwrap_or(&capability.id);
            Some(CapabilityDiagnosticRequest {
                capability_id: capability.id.clone(),
                action: "check".to_string(),
                resource: service_id.to_string(),
                timeout_secs: None,
            })
        }
        CapabilityKind::Process | CapabilityKind::DeviceInfo => None,
    }
}

fn print_report(report: &DoctorReport) {
    print_platform_report(&report.platform);
    for warning in &report.config_warnings {
        println!("config warning: unknown field {warning}");
    }
    for diagnostic in &report.security_diagnostics {
        println!(
            "security {} ok={} severity={} message={} hint={}",
            diagnostic.id, diagnostic.ok, diagnostic.severity, diagnostic.message, diagnostic.hint
        );
    }
    for node in &report.nodes {
        println!(
            "{} endpoint_ok={} health_ok={} protocol_match={}",
            node.node_id,
            node.endpoint_ok,
            node.health_ok,
            node.protocol_match
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        if let Some(error) = &node.endpoint_error {
            println!("  endpoint_error={error}");
        }
        if let Some(error) = &node.health_error {
            println!("  health_error={error}");
        }
        if let Some(error) = &node.capability_error {
            println!("  capability_error={error}");
        }
        if let Some(error) = &node.service_error {
            println!("  service_error={error}");
        }
        for decision in &node.capability_diagnostics {
            println!(
                "  capability {} {} {} allowed={} reason={}",
                decision.capability_id,
                decision.action,
                decision.resource,
                decision.allowed,
                decision.reason_code.as_str()
            );
        }
        for service in &node.service_checks {
            println!(
                "  service {} ok={} latency_ms={} reason={}",
                service.id,
                service.ok,
                service.latency_ms,
                service.reason.as_deref().unwrap_or("-")
            );
        }
    }
}

fn print_platform_report(platform: &DoctorPlatformReport) {
    println!(
        "platform os={} arch={} mount={} mount_runtime={} mount_runtime_ready={} mount_hint={} private_files={} exec_cancel={} pty={} service_forwarding={}",
        platform.os,
        platform.arch,
        platform.mount_adapter,
        platform.mount_runtime,
        platform.mount_runtime_ready,
        platform.mount_hint,
        platform.private_file_protection,
        platform.exec_cancellation,
        platform.pty_sessions,
        platform.service_forwarding
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn capability_diagnostic_uses_policy_actions_for_capability_kind() {
        let request = diagnostic_request_for_capability(&operon_core::Capability {
            id: "service:web".to_string(),
            kind: CapabilityKind::Service,
            node_id: "node-a".to_string(),
            name: "web".to_string(),
            permissions: vec!["check".to_string()],
            description: String::new(),
        })
        .expect("service diagnostic");

        assert_eq!(request.capability_id, "service:web");
        assert_eq!(request.action, "check");
        assert_eq!(request.resource, "web");
    }

    #[test]
    fn platform_report_contains_operator_caveats() {
        let report = platform_report();

        assert!(!report.mount_adapter.is_empty());
        assert!(!report.mount_runtime.is_empty());
        assert_eq!(report.mount_runtime_ready, mount_runtime::report().ready);
        assert!(!report.mount_hint.is_empty());
        #[cfg(target_os = "linux")]
        assert_eq!(report.mount_adapter, "linux-fuse-supported");
        #[cfg(target_os = "macos")]
        assert_eq!(
            report.mount_adapter,
            "macos-fuse-t-supported-runtime-required"
        );
        #[cfg(windows)]
        assert_eq!(
            report.mount_adapter,
            "windows-winfsp-supported-runtime-required"
        );
        assert!(!report.private_file_protection.is_empty());
        #[cfg(windows)]
        assert_eq!(report.private_file_protection, "windows-acl-verified");
        assert!(!report.exec_cancellation.is_empty());
        #[cfg(windows)]
        assert_eq!(report.pty_sessions, "windows-portable-pty-smoke-validated");
        #[cfg(not(windows))]
        assert_eq!(report.pty_sessions, "portable-pty-smoke-validated");
        assert!(report.service_forwarding.contains("firewall"));
    }

    #[test]
    fn security_diagnostics_warn_for_non_loopback_without_auth_and_inline_tokens() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
daemon:
  node_id: local
  grpc_listen: 0.0.0.0:7789
  workspace: /workspace
  auth:
    token: daemon-secret
client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:7789
      auth:
        token: client-secret
policy:
  subject: local
  fs:
    mounts: []
  exec:
    allowed_cwds: []
    default_timeout_secs: 1
    max_timeout_secs: 1
    allow_sessions: false
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: daemon
        name: daemon
        host: 127.0.0.1
        port: 7789
        protocol: tcp
        description: local daemon
"#,
        )
        .expect("config");

        let diagnostics = security_diagnostics(&config, Path::new("."), Path::new("config.yaml"));

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "daemon-non-loopback-auth" && diagnostic.ok));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "daemon-auth-reference"
                && !diagnostic.ok
                && diagnostic.severity == "warning"));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.id
            == "client-node-local-auth-reference"
            && !diagnostic.ok
            && diagnostic.severity == "warning"));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "service-daemon-permissions"
                && !diagnostic.ok
                && diagnostic.severity == "warning"));
    }

    #[test]
    fn security_diagnostics_error_for_non_loopback_without_auth() {
        let config: OperonConfig = serde_yaml::from_str(
            r#"
version: 1
daemon:
  node_id: local
  grpc_listen: 0.0.0.0:7789
  workspace: /workspace
"#,
        )
        .expect("config");

        let diagnostics = security_diagnostics(&config, Path::new("."), Path::new("config.yaml"));

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "daemon-non-loopback-auth"
                && !diagnostic.ok
                && diagnostic.severity == "error"));
    }
}
