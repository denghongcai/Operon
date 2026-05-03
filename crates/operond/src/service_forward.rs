use std::pin::Pin;

use futures_util::{Stream, StreamExt};
use operon_core::{
    PolicyConfig, PolicyDecision, PolicyReasonCode, ServiceCheck, ServiceDefinition,
};
use operon_protocol::runtime::v1::{
    service_datagram_tunnel_request, service_tunnel_request, ServiceDatagramTunnelRequest,
    ServiceTunnelRequest, ServiceTunnelResponse,
};
use tokio::{net::TcpStream, time};
use tonic::Status;

use crate::{
    audit::{record_audit_capability, record_policy_decision},
    grpc_status::status_from_error,
    service_datagram_forward, service_tcp_forward,
    state::AppState,
};

pub(crate) type ServiceTunnelStream =
    Pin<Box<dyn Stream<Item = Result<ServiceTunnelResponse, Status>> + Send + 'static>>;
pub(crate) type ServiceDatagramTunnelStream = service_datagram_forward::ServiceDatagramTunnelStream;

pub(crate) async fn grpc_service_check(
    state: &AppState,
    service_id: String,
) -> Result<ServiceCheck, Status> {
    let service = match authorize_service_decision(&state.policy, &service_id, "check") {
        Ok((service, _)) => service,
        Err(decision) => {
            record_policy_decision(state, &decision);
            return Err(status_from_error(decision.runtime_error()));
        }
    };

    let check = match service.protocol {
        operon_core::ServiceProtocol::Tcp => {
            operon_network::check_tcp_service(&service, std::time::Duration::from_secs(2)).await
        }
        operon_core::ServiceProtocol::Udp => {
            operon_network::check_udp_service(&service, std::time::Duration::from_secs(2)).await
        }
    };
    record_audit_capability(
        state,
        &format!("service:{}", service.id),
        "check",
        &service.id,
        check.ok,
        &service_check_audit_reason(&service, &check),
    );

    Ok(check)
}

#[cfg(test)]
pub(crate) fn authorize_service(
    policy: &PolicyConfig,
    service_id: &str,
    action: &str,
) -> Result<ServiceDefinition, (operon_core::RuntimeErrorKind, String)> {
    authorize_service_decision(policy, service_id, action)
        .map(|(service, _)| service)
        .map_err(|decision| decision.runtime_error())
}

pub(crate) fn authorize_service_decision(
    policy: &PolicyConfig,
    service_id: &str,
    action: &str,
) -> Result<(ServiceDefinition, PolicyDecision), Box<PolicyDecision>> {
    let service = policy
        .service
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| {
            Box::new(PolicyDecision::denied(
                &policy.subject,
                format!("service:{service_id}"),
                action,
                service_id,
                PolicyReasonCode::ServiceUnknown,
                format!("service `{service_id}` denied by policy"),
            ))
        })?;
    let allowed = match action {
        "check" => service.permissions.check,
        "forward" => service.permissions.forward,
        _ => false,
    };
    if allowed {
        Ok((
            service.clone(),
            PolicyDecision::allowed(
                &policy.subject,
                format!("service:{}", service.id),
                action,
                service_id,
                "allowed",
            ),
        ))
    } else {
        let reason_code = match action {
            "check" | "forward" => PolicyReasonCode::ServiceActionDenied,
            _ => PolicyReasonCode::UnsupportedAction,
        };
        Err(Box::new(PolicyDecision::denied(
            &policy.subject,
            format!("service:{}", service.id),
            action,
            service_id,
            reason_code,
            format!("service `{service_id}` action `{action}` denied by policy"),
        )))
    }
}

fn service_check_audit_reason(service: &ServiceDefinition, check: &ServiceCheck) -> String {
    let protocol = match service.protocol {
        operon_core::ServiceProtocol::Tcp => "tcp",
        operon_core::ServiceProtocol::Udp => "udp",
    };
    if check.ok {
        return match &check.reason {
            Some(reason) => format!("{protocol} service reachable: {reason}"),
            None => format!("{protocol} service reachable"),
        };
    }
    match &check.reason {
        Some(reason) => format!("{protocol} service unreachable: {reason}"),
        None => format!("{protocol} service unreachable"),
    }
}

pub(crate) async fn open_service_tunnel(
    state: &AppState,
    mut input: tonic::Streaming<ServiceTunnelRequest>,
) -> Result<ServiceTunnelStream, Status> {
    let first = input
        .next()
        .await
        .ok_or_else(|| Status::invalid_argument("service tunnel target metadata is required"))??;
    let service_id = match first.payload {
        Some(service_tunnel_request::Payload::Target(target)) => {
            if target.service_id.is_empty() {
                return Err(Status::invalid_argument(
                    "service tunnel target service_id is required",
                ));
            }
            target.service_id
        }
        Some(service_tunnel_request::Payload::Data(_)) => {
            return Err(Status::invalid_argument(
                "service tunnel data arrived before target metadata",
            ));
        }
        Some(service_tunnel_request::Payload::Close(_)) | None => {
            return Err(Status::invalid_argument(
                "service tunnel target metadata is required",
            ));
        }
    };
    let service = match authorize_service_decision(&state.policy, &service_id, "forward") {
        Ok((service, _)) => service,
        Err(decision) => {
            record_policy_decision(state, &decision);
            return Err(status_from_error(decision.runtime_error()));
        }
    };
    if !matches!(service.protocol, operon_core::ServiceProtocol::Tcp) {
        record_audit_capability(
            state,
            &format!("service:{}", service.id),
            "forward",
            &service.id,
            false,
            "only TCP services can be forwarded",
        );
        return Err(Status::failed_precondition(
            "only TCP services can be forwarded",
        ));
    }
    let address = format!("{}:{}", service.host, service.port);
    let tcp = match time::timeout(
        std::time::Duration::from_secs(10),
        TcpStream::connect(&address),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            let reason = format!("failed to connect to service: {error}");
            record_audit_capability(
                state,
                &format!("service:{}", service.id),
                "forward",
                &service.id,
                true,
                &reason,
            );
            return Err(Status::unavailable(reason));
        }
        Err(_) => {
            record_audit_capability(
                state,
                &format!("service:{}", service.id),
                "forward",
                &service.id,
                true,
                "service connection timed out",
            );
            return Err(Status::deadline_exceeded("service connection timed out"));
        }
    };
    record_audit_capability(
        state,
        &format!("service:{}", service.id),
        "forward",
        &service.id,
        true,
        "allowed",
    );
    Ok(Box::pin(service_tcp_forward::service_tunnel_stream(
        service, input, tcp,
    )))
}

pub(crate) async fn open_service_datagram_tunnel(
    state: &AppState,
    mut input: tonic::Streaming<ServiceDatagramTunnelRequest>,
) -> Result<ServiceDatagramTunnelStream, Status> {
    let first = input.next().await.ok_or_else(|| {
        Status::invalid_argument("service datagram tunnel target metadata is required")
    })??;
    let service_id = match first.payload {
        Some(service_datagram_tunnel_request::Payload::Target(target)) => {
            if target.service_id.is_empty() {
                return Err(Status::invalid_argument(
                    "service datagram tunnel target service_id is required",
                ));
            }
            target.service_id
        }
        Some(service_datagram_tunnel_request::Payload::Datagram(_)) => {
            return Err(Status::invalid_argument(
                "service datagram arrived before target metadata",
            ));
        }
        Some(service_datagram_tunnel_request::Payload::Close(_)) | None => {
            return Err(Status::invalid_argument(
                "service datagram tunnel target metadata is required",
            ));
        }
    };
    let service = match authorize_service_decision(&state.policy, &service_id, "forward") {
        Ok((service, _)) => service,
        Err(mut decision) => {
            decision.action = "forward-udp".to_string();
            record_policy_decision(state, &decision);
            return Err(status_from_error(decision.runtime_error()));
        }
    };
    if !matches!(service.protocol, operon_core::ServiceProtocol::Udp) {
        record_audit_capability(
            state,
            &format!("service:{}", service.id),
            "forward-udp",
            &service.id,
            false,
            "only UDP services can be forwarded with datagram tunnels",
        );
        return Err(Status::failed_precondition(
            "only UDP services can be forwarded with datagram tunnels",
        ));
    }
    record_audit_capability(
        state,
        &format!("service:{}", service.id),
        "forward-udp",
        &service.id,
        true,
        "allowed",
    );
    Ok(Box::pin(
        service_datagram_forward::service_datagram_tunnel_stream(service, input),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_core::{ServicePermissions, ServiceProtocol};

    #[test]
    fn service_check_audit_reason_names_tcp_success() {
        let service = test_service(ServiceProtocol::Tcp);
        let check = ServiceCheck {
            id: service.id.clone(),
            ok: true,
            latency_ms: 1,
            reason: None,
        };

        assert_eq!(
            service_check_audit_reason(&service, &check),
            "tcp service reachable"
        );
    }

    #[test]
    fn service_check_audit_reason_names_udp_limited_reachability() {
        let service = test_service(ServiceProtocol::Udp);
        let check = ServiceCheck {
            id: service.id.clone(),
            ok: true,
            latency_ms: 1,
            reason: Some("udp socket connected; datagram response not verified".to_string()),
        };

        assert_eq!(
            service_check_audit_reason(&service, &check),
            "udp service reachable: udp socket connected; datagram response not verified"
        );
    }

    #[test]
    fn service_check_audit_reason_names_protocol_failure() {
        let service = test_service(ServiceProtocol::Tcp);
        let check = ServiceCheck {
            id: service.id.clone(),
            ok: false,
            latency_ms: 1,
            reason: Some("connection refused".to_string()),
        };

        assert_eq!(
            service_check_audit_reason(&service, &check),
            "tcp service unreachable: connection refused"
        );
    }

    #[test]
    fn service_authorization_decision_names_reason_codes() {
        let mut policy = PolicyConfig {
            subject: "local-cli".to_string(),
            fs: operon_core::FsPolicy { mounts: Vec::new() },
            exec: operon_core::ExecPolicy {
                allowed_cwds: Vec::new(),
                default_timeout_secs: 30,
                max_timeout_secs: 300,
                allow_sessions: false,
                preserve_env: false,
                env_allowlist: Vec::new(),
                allowed_secrets: Vec::new(),
            },
            service: operon_core::ServicePolicy::default(),
        };
        policy.service.services.push(ServiceDefinition {
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
        });

        let (_, allowed) =
            authorize_service_decision(&policy, "web", "check").expect("check allowed");
        assert!(allowed.allowed);
        assert_eq!(allowed.capability_id, "service:web");
        assert_eq!(allowed.reason_code, operon_core::PolicyReasonCode::Allowed);

        let denied =
            authorize_service_decision(&policy, "web", "forward").expect_err("forward denied");
        assert_eq!(denied.capability_id, "service:web");
        assert_eq!(
            denied.reason_code,
            operon_core::PolicyReasonCode::ServiceActionDenied
        );

        let unknown =
            authorize_service_decision(&policy, "missing", "check").expect_err("missing service");
        assert_eq!(unknown.capability_id, "service:missing");
        assert_eq!(
            unknown.reason_code,
            operon_core::PolicyReasonCode::ServiceUnknown
        );
    }

    fn test_service(protocol: ServiceProtocol) -> ServiceDefinition {
        ServiceDefinition {
            id: "svc".to_string(),
            name: "svc".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            protocol,
            description: String::new(),
            permissions: ServicePermissions {
                check: true,
                forward: true,
            },
        }
    }
}
