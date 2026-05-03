use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use futures_util::{Stream, StreamExt};
use operon_core::ServiceDefinition;
use operon_protocol::runtime::v1::{
    service_datagram_tunnel_request, service_datagram_tunnel_response, ServiceDatagram,
    ServiceDatagramTunnelClose, ServiceDatagramTunnelOpened, ServiceDatagramTunnelRequest,
    ServiceDatagramTunnelResponse,
};
use tokio::{net::UdpSocket, sync::mpsc, task::JoinHandle, time};
use tonic::Status;

use crate::{MAX_SERVICE_DATAGRAM_BYTES, SERVICE_DATAGRAM_PEER_IDLE_SECS};

pub(crate) type ServiceDatagramTunnelStream =
    Pin<Box<dyn Stream<Item = Result<ServiceDatagramTunnelResponse, Status>> + Send + 'static>>;

struct ServiceDatagramPeerSession {
    socket: Arc<UdpSocket>,
    read_task: JoinHandle<()>,
    last_seen: time::Instant,
    packets_from_client: u64,
    bytes_from_client: u64,
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
