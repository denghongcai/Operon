//! Experimental HTTP/2 over an authenticated iroh bidirectional stream.
//! No public relays, discovery publication, TCP proxy or production changes.
use hyper_util::rt::TokioIo;
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tonic::transport::{Channel, Endpoint as GrpcEndpoint, server::Connected};

pub const ALPN: &[u8] = b"operon/poc/grpc-h2/1";

pub mod probe {
    tonic::include_proto!("operon.poc");
}
pub mod runtime {
    tonic::include_proto!("operon.runtime.v1");
}

pub async fn local_endpoint() -> anyhow::Result<Endpoint> {
    Ok(Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?)
}

/// Network-reachable endpoint for cross-container tests: binds all interfaces
/// with IP transports kept, and never uses relays or discovery lookups.
pub async fn network_endpoint() -> anyhow::Result<Endpoint> {
    Ok(Endpoint::builder(presets::Minimal)
        .clear_relay_transports()
        .bind_addr("0.0.0.0:0")?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?)
}

/// Minimal shared gRPC service used by the container examples.
pub mod demo {
    use crate::probe::{Frame, probe_server::Probe};
    use std::pin::Pin;
    use tokio_stream::Stream;
    use tonic::{Request, Response, Status, Streaming};

    #[derive(Clone, Default)]
    pub struct Echo;
    #[tonic::async_trait]
    impl Probe for Echo {
        async fn echo(&self, request: Request<Frame>) -> Result<Response<Frame>, Status> {
            Ok(Response::new(request.into_inner()))
        }
        async fn hang(&self, _: Request<Frame>) -> Result<Response<Frame>, Status> {
            Err(Status::unimplemented("hang is transport-test only"))
        }
        type DuplexStream = Pin<Box<dyn Stream<Item = Result<Frame, Status>> + Send>>;
        async fn duplex(
            &self,
            request: Request<Streaming<Frame>>,
        ) -> Result<Response<Self::DuplexStream>, Status> {
            let mut input = request.into_inner();
            Ok(Response::new(Box::pin(async_stream::try_stream! {
                while let Some(frame) = input.message().await? { yield frame; }
            })))
        }
    }
}

/// Retain the connection alongside its stream; stream handles alone do not
/// express the application-owned connection lifetime.
pub struct QuicIo {
    pub connection: Connection,
    send: SendStream,
    recv: RecvStream,
}
impl QuicIo {
    pub fn new(connection: Connection, send: SendStream, recv: RecvStream) -> Self {
        Self {
            connection,
            send,
            recv,
        }
    }
}
impl AsyncRead for QuicIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}
impl AsyncWrite for QuicIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.send), cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_shutdown(cx)
    }
}
impl Connected for QuicIo {
    type ConnectInfo = EndpointId;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.connection.remote_id()
    }
}

pub async fn channel(
    endpoint: Endpoint,
    target: EndpointAddr,
    timeout: Duration,
) -> anyhow::Result<Channel> {
    // This URI is only an HTTP authority, not DNS or a TCP destination.
    let connector = tower::service_fn(move |_| {
        let endpoint = endpoint.clone();
        let target = target.clone();
        async move {
            let connection = endpoint
                .connect(target, ALPN)
                .await
                .map_err(io::Error::other)?;
            let (send, recv) = connection.open_bi().await.map_err(io::Error::other)?;
            Ok::<_, io::Error>(TokioIo::new(QuicIo::new(connection, send, recv)))
        }
    });
    let mut grpc = GrpcEndpoint::from_static("http://iroh.invalid");
    if !timeout.is_zero() {
        grpc = grpc.connect_timeout(timeout);
    }
    Ok(grpc.connect_with_connector(connector).await?)
}
