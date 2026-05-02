use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use futures_util::{Stream, StreamExt};
use operon_core::{
    PolicyConfig, PolicyDecision, PolicyReasonCode, ServiceCheck, ServiceDefinition,
};
use operon_protocol::runtime::v1::{
    service_datagram_tunnel_request, service_datagram_tunnel_response, service_tunnel_request,
    service_tunnel_response, ServiceDatagram, ServiceDatagramTunnelClose,
    ServiceDatagramTunnelOpened, ServiceDatagramTunnelRequest, ServiceDatagramTunnelResponse,
    ServiceTunnelClose, ServiceTunnelData, ServiceTunnelOpened, ServiceTunnelRequest,
    ServiceTunnelResponse,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::mpsc,
    task::JoinHandle,
    time,
};
use tonic::Status;

use crate::{
    audit::{record_audit_capability, record_policy_decision},
    grpc_status::status_from_error,
    state::AppState,
    MAX_SERVICE_DATAGRAM_BYTES, SERVICE_DATAGRAM_PEER_IDLE_SECS,
};

pub(crate) type ServiceTunnelStream =
    Pin<Box<dyn Stream<Item = Result<ServiceTunnelResponse, Status>> + Send + 'static>>;
pub(crate) type ServiceDatagramTunnelStream =
    Pin<Box<dyn Stream<Item = Result<ServiceDatagramTunnelResponse, Status>> + Send + 'static>>;

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
    Ok(Box::pin(service_tunnel_stream(service, input, tcp)))
}

pub(crate) fn service_tunnel_stream(
    service: ServiceDefinition,
    mut input: tonic::Streaming<ServiceTunnelRequest>,
    tcp: TcpStream,
) -> impl futures_util::Stream<Item = Result<ServiceTunnelResponse, Status>> + Send + 'static {
    async_stream::try_stream! {
        let (mut remote_reader, mut remote_writer) = tcp.into_split();
        yield ServiceTunnelResponse {
            payload: Some(service_tunnel_response::Payload::Opened(ServiceTunnelOpened {
                service_id: service.id.clone(),
                host: service.host.clone(),
                port: u32::from(service.port),
            })),
        };

        let mut client_open = true;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            tokio::select! {
                message = input.next(), if client_open => {
                    match message {
                        Some(Ok(message)) => match message.payload {
                            Some(service_tunnel_request::Payload::Data(data)) => {
                                if data.data.is_empty() {
                                    continue;
                                }
                                if let Err(error) = remote_writer.write_all(&data.data).await {
                                    yield ServiceTunnelResponse {
                                        payload: Some(service_tunnel_response::Payload::Close(ServiceTunnelClose {
                                            reason: format!("remote service write failed: {error}"),
                                        })),
                                    };
                                    break;
                                }
                            }
                            Some(service_tunnel_request::Payload::Close(close)) => {
                                let _ = remote_writer.shutdown().await;
                                client_open = false;
                                if !close.reason.is_empty() {
                                    tracing::debug!("service tunnel client closed: {}", close.reason);
                                }
                            }
                            Some(service_tunnel_request::Payload::Target(_)) => {
                                yield ServiceTunnelResponse {
                                    payload: Some(service_tunnel_response::Payload::Close(ServiceTunnelClose {
                                        reason: "service tunnel target metadata was sent more than once".to_string(),
                                    })),
                                };
                                break;
                            }
                            None => {}
                        },
                        Some(Err(status)) => {
                            yield ServiceTunnelResponse {
                                payload: Some(service_tunnel_response::Payload::Close(ServiceTunnelClose {
                                    reason: status.message().to_string(),
                                })),
                            };
                            break;
                        }
                        None => {
                            let _ = remote_writer.shutdown().await;
                            client_open = false;
                        }
                    }
                }
                read_result = remote_reader.read(&mut buffer) => {
                    match read_result {
                        Ok(0) => {
                            yield ServiceTunnelResponse {
                                payload: Some(service_tunnel_response::Payload::Close(ServiceTunnelClose {
                                    reason: "remote service closed".to_string(),
                                })),
                            };
                            break;
                        }
                        Ok(bytes_read) => {
                            yield ServiceTunnelResponse {
                                payload: Some(service_tunnel_response::Payload::Data(ServiceTunnelData {
                                    data: buffer[..bytes_read].to_vec(),
                                })),
                            };
                        }
                        Err(error) => {
                            yield ServiceTunnelResponse {
                                payload: Some(service_tunnel_response::Payload::Close(ServiceTunnelClose {
                                    reason: format!("remote service read failed: {error}"),
                                })),
                            };
                            break;
                        }
                    }
                }
            }
        }
    }
}

struct ServiceDatagramPeerSession {
    socket: Arc<UdpSocket>,
    read_task: JoinHandle<()>,
    last_seen: time::Instant,
    packets_from_client: u64,
    bytes_from_client: u64,
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
    Ok(Box::pin(service_datagram_tunnel_stream(service, input)))
}

pub(crate) fn service_datagram_tunnel_stream(
    service: ServiceDefinition,
    input: tonic::Streaming<ServiceDatagramTunnelRequest>,
) -> impl futures_util::Stream<Item = Result<ServiceDatagramTunnelResponse, Status>> + Send + 'static
{
    async_stream::stream! {
        let (output_tx, mut output_rx) = mpsc::unbounded_channel();
        tokio::spawn(run_service_datagram_tunnel(service, input, output_tx));
        while let Some(message) = output_rx.recv().await {
            yield message;
        }
    }
}

async fn run_service_datagram_tunnel(
    service: ServiceDefinition,
    mut input: tonic::Streaming<ServiceDatagramTunnelRequest>,
    output_tx: mpsc::UnboundedSender<Result<ServiceDatagramTunnelResponse, Status>>,
) {
    let _ = output_tx.send(Ok(ServiceDatagramTunnelResponse {
        payload: Some(service_datagram_tunnel_response::Payload::Opened(
            ServiceDatagramTunnelOpened {
                service_id: service.id.clone(),
                host: service.host.clone(),
                port: u32::from(service.port),
            },
        )),
    }));

    let address = format!("{}:{}", service.host, service.port);
    let mut sessions: BTreeMap<String, ServiceDatagramPeerSession> = BTreeMap::new();
    let mut cleanup_interval = time::interval(std::time::Duration::from_secs(5));
    let idle_timeout = std::time::Duration::from_secs(SERVICE_DATAGRAM_PEER_IDLE_SECS);

    loop {
        tokio::select! {
            message = input.next() => {
                match message {
                    Some(Ok(message)) => {
                        if !handle_service_datagram_message(
                            message,
                            &address,
                            &mut sessions,
                            &output_tx,
                        ).await {
                            break;
                        }
                    }
                    Some(Err(status)) => {
                        let _ = output_tx.send(Err(status));
                        break;
                    }
                    None => break,
                }
            }
            _ = cleanup_interval.tick() => {
                prune_idle_datagram_sessions(&mut sessions, &output_tx, idle_timeout);
            }
        }
    }

    for (_, session) in sessions {
        session.read_task.abort();
    }
    let _ = output_tx.send(Ok(ServiceDatagramTunnelResponse {
        payload: Some(service_datagram_tunnel_response::Payload::Close(
            ServiceDatagramTunnelClose {
                peer_id: String::new(),
                reason: "datagram tunnel closed".to_string(),
            },
        )),
    }));
}

async fn handle_service_datagram_message(
    message: ServiceDatagramTunnelRequest,
    address: &str,
    sessions: &mut BTreeMap<String, ServiceDatagramPeerSession>,
    output_tx: &mpsc::UnboundedSender<Result<ServiceDatagramTunnelResponse, Status>>,
) -> bool {
    match message.payload {
        Some(service_datagram_tunnel_request::Payload::Datagram(datagram)) => {
            if datagram.peer_id.is_empty() {
                let _ = output_tx.send(Err(Status::invalid_argument(
                    "service datagram peer_id is required",
                )));
                return false;
            }
            if datagram.data.len() > MAX_SERVICE_DATAGRAM_BYTES {
                send_service_datagram_close(
                    output_tx,
                    &datagram.peer_id,
                    "service datagram exceeds maximum UDP payload size",
                );
                return true;
            }
            if !sessions.contains_key(&datagram.peer_id) {
                match create_service_datagram_session(&datagram.peer_id, address, output_tx).await {
                    Ok(session) => {
                        sessions.insert(datagram.peer_id.clone(), session);
                    }
                    Err(error) => {
                        send_service_datagram_close(
                            output_tx,
                            &datagram.peer_id,
                            &format!("failed to connect UDP service: {error}"),
                        );
                        return true;
                    }
                }
            }
            let socket = {
                let Some(session) = sessions.get_mut(&datagram.peer_id) else {
                    send_service_datagram_close(
                        output_tx,
                        &datagram.peer_id,
                        "service datagram session is missing",
                    );
                    return true;
                };
                session.last_seen = time::Instant::now();
                session.packets_from_client = session.packets_from_client.saturating_add(1);
                session.bytes_from_client = session
                    .bytes_from_client
                    .saturating_add(datagram.data.len() as u64);
                session.socket.clone()
            };
            if let Err(error) = socket.send(&datagram.data).await {
                if let Some(session) = sessions.remove(&datagram.peer_id) {
                    session.read_task.abort();
                }
                send_service_datagram_close(
                    output_tx,
                    &datagram.peer_id,
                    &format!("failed to send UDP datagram: {error}"),
                );
            }
            true
        }
        Some(service_datagram_tunnel_request::Payload::Close(close)) => {
            if close.peer_id.is_empty() {
                return false;
            }
            if let Some(session) = sessions.remove(&close.peer_id) {
                session.read_task.abort();
            }
            send_service_datagram_close(
                output_tx,
                &close.peer_id,
                if close.reason.is_empty() {
                    "peer closed"
                } else {
                    &close.reason
                },
            );
            true
        }
        Some(service_datagram_tunnel_request::Payload::Target(_)) => {
            let _ = output_tx.send(Err(Status::invalid_argument(
                "service datagram tunnel target metadata was sent more than once",
            )));
            false
        }
        None => true,
    }
}

async fn create_service_datagram_session(
    peer_id: &str,
    address: &str,
    output_tx: &mpsc::UnboundedSender<Result<ServiceDatagramTunnelResponse, Status>>,
) -> std::io::Result<ServiceDatagramPeerSession> {
    let socket = Arc::new(connect_udp_socket(address).await?);
    let read_socket = socket.clone();
    let peer_id = peer_id.to_string();
    let output_tx = output_tx.clone();
    let read_task = tokio::spawn(async move {
        let mut buffer = vec![0_u8; MAX_SERVICE_DATAGRAM_BYTES];
        loop {
            match read_socket.recv(&mut buffer).await {
                Ok(bytes_read) => {
                    let _ = output_tx.send(Ok(ServiceDatagramTunnelResponse {
                        payload: Some(service_datagram_tunnel_response::Payload::Datagram(
                            ServiceDatagram {
                                peer_id: peer_id.clone(),
                                data: buffer[..bytes_read].to_vec(),
                            },
                        )),
                    }));
                }
                Err(error) => {
                    send_service_datagram_close(
                        &output_tx,
                        &peer_id,
                        &format!("failed to receive UDP datagram: {error}"),
                    );
                    break;
                }
            }
        }
    });

    Ok(ServiceDatagramPeerSession {
        socket,
        read_task,
        last_seen: time::Instant::now(),
        packets_from_client: 0,
        bytes_from_client: 0,
    })
}

async fn connect_udp_socket(address: &str) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    match socket.connect(address).await {
        Ok(()) => Ok(socket),
        Err(ipv4_error) => {
            let socket = UdpSocket::bind("[::]:0").await?;
            socket.connect(address).await.map_err(|_| ipv4_error)?;
            Ok(socket)
        }
    }
}

fn prune_idle_datagram_sessions(
    sessions: &mut BTreeMap<String, ServiceDatagramPeerSession>,
    output_tx: &mpsc::UnboundedSender<Result<ServiceDatagramTunnelResponse, Status>>,
    idle_timeout: std::time::Duration,
) {
    let now = time::Instant::now();
    let idle_peer_ids = sessions
        .iter()
        .filter(|(_, session)| now.duration_since(session.last_seen) > idle_timeout)
        .map(|(peer_id, _)| peer_id.clone())
        .collect::<Vec<_>>();
    for peer_id in idle_peer_ids {
        if let Some(session) = sessions.remove(&peer_id) {
            session.read_task.abort();
        }
        send_service_datagram_close(output_tx, &peer_id, "peer session idle timeout");
    }
}

fn send_service_datagram_close(
    output_tx: &mpsc::UnboundedSender<Result<ServiceDatagramTunnelResponse, Status>>,
    peer_id: &str,
    reason: &str,
) {
    let _ = output_tx.send(Ok(ServiceDatagramTunnelResponse {
        payload: Some(service_datagram_tunnel_response::Payload::Close(
            ServiceDatagramTunnelClose {
                peer_id: peer_id.to_string(),
                reason: reason.to_string(),
            },
        )),
    }));
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
            job: operon_core::JobPolicy {
                allowed_cwds: Vec::new(),
                default_timeout_secs: 30,
                max_timeout_secs: 300,
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
