use std::{path::PathBuf, time::Duration};

use operon_config::OperonConfig;
use operon_core::{CapabilityDiagnosticRequest, CapabilityKind, PolicyDecision, ServiceCheck};

use crate::{
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
    nodes: Vec<DoctorNodeReport>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorPlatformReport {
    os: String,
    arch: String,
    mount_adapter: String,
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
    output: OutputMode,
) -> anyhow::Result<()> {
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
    DoctorPlatformReport {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        mount_adapter: mount_adapter_diagnostic().to_string(),
        private_file_protection: private_file_protection_diagnostic().to_string(),
        exec_cancellation: exec_cancellation_diagnostic().to_string(),
        pty_sessions: pty_session_diagnostic().to_string(),
        service_forwarding: "service forwarding depends on local and remote firewall policy"
            .to_string(),
    }
}

#[cfg(target_os = "linux")]
fn mount_adapter_diagnostic() -> &'static str {
    "linux-fuse-supported"
}

#[cfg(target_os = "macos")]
fn mount_adapter_diagnostic() -> &'static str {
    "macos-fuse-t-supported-runtime-required"
}

#[cfg(windows)]
fn mount_adapter_diagnostic() -> &'static str {
    "windows-winfsp-supported-runtime-required"
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos"), not(windows)))]
fn mount_adapter_diagnostic() -> &'static str {
    "mount-adapter-unsupported-platform"
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
    endpoint: operon_network::NodeEndpoint,
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
    println!(
        "platform os={} arch={} mount={} private_files={} exec_cancel={} pty={} service_forwarding={}",
        report.platform.os,
        report.platform.arch,
        report.platform.mount_adapter,
        report.platform.private_file_protection,
        report.platform.exec_cancellation,
        report.platform.pty_sessions,
        report.platform.service_forwarding
    );
    for warning in &report.config_warnings {
        println!("config warning: unknown field {warning}");
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
