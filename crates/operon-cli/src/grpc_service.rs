use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{
    service_datagram_tunnel_request, service_datagram_tunnel_response, service_tunnel_request,
    service_tunnel_response, ServiceDatagram, ServiceDatagramTunnelRequest,
    ServiceDatagramTunnelTarget, ServiceTunnelClose, ServiceTunnelData, ServiceTunnelRequest,
    ServiceTunnelTarget,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::mpsc,
};

use crate::grpc::{call, with_auth};

pub async fn forward_service_connection(
    endpoint: &NodeEndpoint,
    service_id: &str,
    socket: TcpStream,
) -> anyhow::Result<()> {
    let service_id = service_id.to_string();
    let (mut local_reader, mut local_writer) = socket.into_split();
    call(endpoint, |mut client, endpoint| async move {
        let outbound = async_stream::stream! {
            yield ServiceTunnelRequest {
                payload: Some(service_tunnel_request::Payload::Target(ServiceTunnelTarget {
                    service_id,
                })),
            };
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                match local_reader.read(&mut buffer).await {
                    Ok(0) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Close(ServiceTunnelClose {
                                reason: "local client closed".to_string(),
                            })),
                        };
                        break;
                    }
                    Ok(bytes_read) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Data(ServiceTunnelData {
                                data: buffer[..bytes_read].to_vec(),
                            })),
                        };
                    }
                    Err(error) => {
                        yield ServiceTunnelRequest {
                            payload: Some(service_tunnel_request::Payload::Close(ServiceTunnelClose {
                                reason: format!("local client read failed: {error}"),
                            })),
                        };
                        break;
                    }
                }
            }
        };
        let mut inbound = client
            .open_service_tunnel(with_auth(&endpoint, outbound)?)
            .await?
            .into_inner();
        while let Some(message) = inbound.message().await? {
            match message.payload {
                Some(service_tunnel_response::Payload::Opened(_)) => {}
                Some(service_tunnel_response::Payload::Data(data)) => {
                    local_writer.write_all(&data.data).await?;
                }
                Some(service_tunnel_response::Payload::Close(_)) | None => break,
            }
        }
        let _ = local_writer.shutdown().await;
        Ok(())
    })
    .await
}

#[derive(Debug, Default)]
struct DatagramPeerState {
    next_peer_id: u64,
    addr_to_peer: BTreeMap<SocketAddr, String>,
    peer_to_addr: BTreeMap<String, SocketAddr>,
}

pub async fn forward_service_datagrams(
    endpoint: &NodeEndpoint,
    service_id: &str,
    socket: UdpSocket,
) -> anyhow::Result<()> {
    let service_id = service_id.to_string();
    let socket = Arc::new(socket);
    call(endpoint, |mut client, endpoint| async move {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        request_tx
            .send(ServiceDatagramTunnelRequest {
                payload: Some(service_datagram_tunnel_request::Payload::Target(
                    ServiceDatagramTunnelTarget { service_id },
                )),
            })
            .map_err(|_| anyhow::anyhow!("failed to open UDP datagram tunnel request stream"))?;

        let peer_state = Arc::new(Mutex::new(DatagramPeerState::default()));
        let local_reader = socket.clone();
        let local_request_tx = request_tx.clone();
        let local_peer_state = peer_state.clone();
        let local_read_task = tokio::spawn(async move {
            let mut buffer = vec![0_u8; 65_507];
            loop {
                let Ok((bytes_read, peer_addr)) = local_reader.recv_from(&mut buffer).await else {
                    break;
                };
                let peer_id = match datagram_peer_id(&local_peer_state, peer_addr) {
                    Ok(peer_id) => peer_id,
                    Err(_) => break,
                };
                if local_request_tx
                    .send(ServiceDatagramTunnelRequest {
                        payload: Some(service_datagram_tunnel_request::Payload::Datagram(
                            ServiceDatagram {
                                peer_id,
                                data: buffer[..bytes_read].to_vec(),
                            },
                        )),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        let outbound = async_stream::stream! {
            while let Some(request) = request_rx.recv().await {
                yield request;
            }
        };
        let request = match with_auth(&endpoint, outbound) {
            Ok(request) => request,
            Err(error) => {
                abort_and_wait(local_read_task).await;
                return Err(error);
            }
        };
        let response = match client.open_service_datagram_tunnel(request).await {
            Ok(response) => response,
            Err(error) => {
                abort_and_wait(local_read_task).await;
                return Err(error.into());
            }
        };
        let mut inbound = response.into_inner();
        while let Some(message) = inbound.message().await? {
            match message.payload {
                Some(service_datagram_tunnel_response::Payload::Opened(_)) => {}
                Some(service_datagram_tunnel_response::Payload::Datagram(datagram)) => {
                    let peer_addr = peer_addr_for_id(&peer_state, &datagram.peer_id)?;
                    if let Some(peer_addr) = peer_addr {
                        socket.send_to(&datagram.data, peer_addr).await?;
                    }
                }
                Some(service_datagram_tunnel_response::Payload::Close(close)) => {
                    if close.peer_id.is_empty() {
                        break;
                    }
                    remove_datagram_peer(&peer_state, &close.peer_id)?;
                }
                None => {}
            }
        }
        abort_and_wait(local_read_task).await;
        Ok(())
    })
    .await
}

async fn abort_and_wait<T>(task: tokio::task::JoinHandle<T>) {
    task.abort();
    let _ = task.await;
}

fn datagram_peer_id(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_addr: SocketAddr,
) -> anyhow::Result<String> {
    let mut state = peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?;
    if let Some(peer_id) = state.addr_to_peer.get(&peer_addr) {
        return Ok(peer_id.clone());
    }
    state.next_peer_id = state.next_peer_id.saturating_add(1);
    let peer_id = format!("peer-{}", state.next_peer_id);
    state.addr_to_peer.insert(peer_addr, peer_id.clone());
    state.peer_to_addr.insert(peer_id.clone(), peer_addr);
    Ok(peer_id)
}

fn peer_addr_for_id(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_id: &str,
) -> anyhow::Result<Option<SocketAddr>> {
    Ok(peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?
        .peer_to_addr
        .get(peer_id)
        .copied())
}

fn remove_datagram_peer(
    peer_state: &Arc<Mutex<DatagramPeerState>>,
    peer_id: &str,
) -> anyhow::Result<()> {
    let mut state = peer_state
        .lock()
        .map_err(|_| anyhow::anyhow!("datagram peer state poisoned"))?;
    if let Some(peer_addr) = state.peer_to_addr.remove(peer_id) {
        state.addr_to_peer.remove(&peer_addr);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datagram_peer_state_reuses_peer_id_for_same_address() {
        let peer_state = Arc::new(Mutex::new(DatagramPeerState::default()));
        let peer_addr: SocketAddr = "127.0.0.1:12345".parse().expect("peer addr");

        let first = datagram_peer_id(&peer_state, peer_addr).expect("first peer");
        let second = datagram_peer_id(&peer_state, peer_addr).expect("second peer");

        assert_eq!(first, "peer-1");
        assert_eq!(first, second);
        assert_eq!(
            peer_addr_for_id(&peer_state, &first).expect("lookup"),
            Some(peer_addr)
        );
    }

    #[test]
    fn datagram_peer_state_removes_reverse_mapping() {
        let peer_state = Arc::new(Mutex::new(DatagramPeerState::default()));
        let peer_addr: SocketAddr = "127.0.0.1:12345".parse().expect("peer addr");
        let peer_id = datagram_peer_id(&peer_state, peer_addr).expect("peer");

        remove_datagram_peer(&peer_state, &peer_id).expect("remove peer");

        assert_eq!(
            peer_addr_for_id(&peer_state, &peer_id).expect("lookup"),
            None
        );
    }
}
