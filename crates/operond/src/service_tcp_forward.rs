use futures_util::StreamExt;
use operon_core::ServiceDefinition;
use operon_protocol::runtime::v1::{
    service_tunnel_request, service_tunnel_response, ServiceTunnelClose, ServiceTunnelData,
    ServiceTunnelOpened, ServiceTunnelRequest, ServiceTunnelResponse,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tonic::Status;

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
