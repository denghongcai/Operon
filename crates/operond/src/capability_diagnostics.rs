use std::collections::BTreeMap;

use operon_core::{CapabilityDiagnosticRequest, PolicyConfig, PolicyDecision, PolicyReasonCode};

pub(crate) fn explain_capability_decision(
    policy: &PolicyConfig,
    secrets: &BTreeMap<String, String>,
    request: &CapabilityDiagnosticRequest,
) -> PolicyDecision {
    match capability_family(&request.capability_id) {
        "fs" => explain_fs(policy, request),
        "exec" => explain_exec(policy, request),
        "secret" => explain_secret(policy, secrets, request),
        "service" => explain_service(policy, request),
        _ => unsupported(policy, request),
    }
}

fn capability_family(capability_id: &str) -> &str {
    capability_id
        .split_once(':')
        .map(|(family, _)| family)
        .unwrap_or(capability_id)
}

fn explain_fs(policy: &PolicyConfig, request: &CapabilityDiagnosticRequest) -> PolicyDecision {
    if !matches!(request.action.as_str(), "read" | "write" | "delete") {
        return unsupported(policy, request);
    }
    operon_fs::authorize_fs_decision(policy, &request.action, &request.resource)
}

fn explain_exec(policy: &PolicyConfig, request: &CapabilityDiagnosticRequest) -> PolicyDecision {
    match request.action.as_str() {
        "run" => operon_process::authorize_exec_decision(
            &policy.subject,
            &policy.exec,
            &request.resource,
            request.timeout_secs,
        ),
        "session" => operon_process::authorize_exec_session_decision(
            &policy.subject,
            &policy.exec,
            &request.resource,
            request.timeout_secs,
        ),
        _ => unsupported(policy, request),
    }
}

fn explain_secret(
    policy: &PolicyConfig,
    secrets: &BTreeMap<String, String>,
    request: &CapabilityDiagnosticRequest,
) -> PolicyDecision {
    if request.action != "use" {
        return unsupported(policy, request);
    }
    match operon_process::resolve_exec_secrets_decision(
        &policy.subject,
        &policy.exec,
        secrets,
        std::slice::from_ref(&request.resource),
    ) {
        Ok(_) => PolicyDecision::allowed(
            &policy.subject,
            "secret:default",
            "use",
            &request.resource,
            "allowed",
        ),
        Err(decision) => *decision,
    }
}

fn explain_service(policy: &PolicyConfig, request: &CapabilityDiagnosticRequest) -> PolicyDecision {
    match crate::service_forward::authorize_service_decision(
        policy,
        &request.resource,
        &request.action,
    ) {
        Ok((_, decision)) => decision,
        Err(decision) => *decision,
    }
}

fn unsupported(policy: &PolicyConfig, request: &CapabilityDiagnosticRequest) -> PolicyDecision {
    PolicyDecision::denied(
        &policy.subject,
        &request.capability_id,
        &request.action,
        &request.resource,
        PolicyReasonCode::UnsupportedAction,
        format!(
            "capability `{}` action `{}` is not supported for diagnostics",
            request.capability_id, request.action
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use operon_core::{
        CapabilityDiagnosticRequest, ExecPolicy, FsMountPolicy, FsPermissions, FsPolicy,
        PolicyConfig, PolicyReasonCode, ServiceDefinition, ServicePermissions, ServicePolicy,
        ServiceProtocol,
    };

    use super::explain_capability_decision;

    fn test_policy() -> PolicyConfig {
        PolicyConfig {
            subject: "local-cli".to_string(),
            fs: FsPolicy {
                mounts: vec![FsMountPolicy {
                    name: "workspace".to_string(),
                    path: "/workspace".to_string(),
                    permissions: FsPermissions {
                        read: true,
                        write: false,
                        delete: false,
                    },
                }],
            },
            exec: ExecPolicy {
                allowed_cwds: vec!["/workspace".to_string()],
                default_timeout_secs: 30,
                max_timeout_secs: 60,
                allow_sessions: false,
                preserve_env: false,
                env_allowlist: Vec::new(),
                allowed_secrets: vec!["TOKEN".to_string()],
            },
            service: ServicePolicy {
                services: vec![ServiceDefinition {
                    id: "web".to_string(),
                    name: "web".to_string(),
                    host: "127.0.0.1".to_string(),
                    port: 8080,
                    protocol: ServiceProtocol::Tcp,
                    description: "web".to_string(),
                    permissions: ServicePermissions {
                        check: true,
                        forward: false,
                    },
                }],
            },
        }
    }

    #[test]
    fn capability_diagnostics_explain_allowed_fs_read() {
        let decision = explain_capability_decision(
            &test_policy(),
            &BTreeMap::new(),
            &CapabilityDiagnosticRequest {
                capability_id: "fs:workspace".to_string(),
                action: "read".to_string(),
                resource: "/workspace/file.txt".to_string(),
                timeout_secs: None,
            },
        );

        assert!(decision.allowed);
        assert_eq!(decision.capability_id, "fs:workspace");
        assert_eq!(decision.reason_code, PolicyReasonCode::Allowed);
    }

    #[test]
    fn capability_diagnostics_explain_exec_timeout_denial() {
        let decision = explain_capability_decision(
            &test_policy(),
            &BTreeMap::new(),
            &CapabilityDiagnosticRequest {
                capability_id: "exec:default".to_string(),
                action: "run".to_string(),
                resource: "/workspace".to_string(),
                timeout_secs: Some(61),
            },
        );

        assert!(!decision.allowed);
        assert_eq!(decision.capability_id, "exec:default");
        assert_eq!(decision.reason_code, PolicyReasonCode::ExecTimeoutExceeded);
    }

    #[test]
    fn capability_diagnostics_explain_secret_undefined() {
        let decision = explain_capability_decision(
            &test_policy(),
            &BTreeMap::new(),
            &CapabilityDiagnosticRequest {
                capability_id: "secret:default".to_string(),
                action: "use".to_string(),
                resource: "TOKEN".to_string(),
                timeout_secs: None,
            },
        );

        assert!(!decision.allowed);
        assert_eq!(decision.reason_code, PolicyReasonCode::SecretUndefined);
    }

    #[test]
    fn capability_diagnostics_explain_unsupported_action() {
        let decision = explain_capability_decision(
            &test_policy(),
            &BTreeMap::new(),
            &CapabilityDiagnosticRequest {
                capability_id: "service:web".to_string(),
                action: "restart".to_string(),
                resource: "web".to_string(),
                timeout_secs: None,
            },
        );

        assert!(!decision.allowed);
        assert_eq!(decision.capability_id, "service:web");
        assert_eq!(decision.reason_code, PolicyReasonCode::UnsupportedAction);
    }
}
