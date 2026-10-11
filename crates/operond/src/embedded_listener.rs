//! Native mixed TCP/iroh incoming for the same capability/auth/policy router.

use futures_util::StreamExt;
use operon_config::IrohListenerConfig;
use operon_transport::embedded::{EmbeddedEndpoint, EndpointId, QuicIo};
use std::{
    future::Future,
    io,
    net::SocketAddr,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tonic::transport::server::{Connected, Router};

pub(crate) async fn serve(
    router: Router,
    listen: SocketAddr,
    config: &IrohListenerConfig,
    config_dir: &Path,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    // TCP bind failures preserve the ordinary startup diagnostic and never
    // silently switch the advertised default listener to another backend.
    let tcp = TcpListener::bind(listen).await?;
    tokio::pin!(shutdown);
    let endpoint = tokio::select! {
        _ = &mut shutdown => return Ok(()),
        result = EmbeddedEndpoint::from_config(&config.endpoint, config_dir) => result?,
    };
    if config.endpoint.bind_addrs.is_empty() {
        let readiness = tokio::select! {
            _ = &mut shutdown => {
                endpoint.close().await;
                return Ok(());
            },
            result = endpoint.wait_online(Duration::from_secs(config.endpoint.readiness_timeout_secs)) => result,
        };
        if let Err(error) = readiness {
            endpoint.close().await;
            return Err(error.into());
        }
    }
    tracing::info!(endpoint_id = %endpoint.id(), addresses = ?endpoint.addr().addrs,
        "operond optional iroh listener ready");
    let mut incoming = match endpoint.incoming(
        config.max_connections,
        Duration::from_secs(config.handshake_timeout_secs),
    ) {
        Ok(incoming) => incoming,
        Err(error) => {
            endpoint.close().await;
            return Err(error.into());
        }
    };
    let (listener_failed, mut listener_failure) = tokio::sync::oneshot::channel();
    let incoming_ref = &mut incoming;
    let stream = async_stream::stream! {
        loop {
            tokio::select! {
                accepted = tcp.accept() => {
                    match accepted {
                        Ok((socket, _)) => {
                            if let Err(error) = socket.set_nodelay(true) { yield Err(error); continue; }
                            yield Ok(RuntimeIo::Tcp(socket));
                        },
                        Err(error) => yield Err(error),
                    }
                },
                accepted = incoming_ref.next() => {
                    match accepted {
                        Some(io) => yield io.map(RuntimeIo::Iroh),
                        None => {
                            // Tonic treats incoming errors as per-connection errors.
                            // Signal its shutdown future explicitly on listener loss.
                            let _ = listener_failed.send(());
                            break;
                        },
                    }
                },
            }
        }
    };
    let mut listener_lost = false;
    let result = router
        .serve_with_incoming_shutdown(stream, async {
            tokio::select! {
                biased;
                _ = shutdown.as_mut() => {},
                failure = &mut listener_failure => {
                    listener_lost = failure.is_ok();
                },
            }
            // Do not leave zero-deadline RPC streams pending on a stopped peer.
            endpoint.close().await;
        })
        .await;
    // Serve drops the borrowed incoming stream before returning; close/abort
    // and join admissions even when the router returned an error.
    let admissions = incoming.shutdown().await;
    endpoint.close().await;
    admissions?;
    result?;
    anyhow::ensure!(
        !listener_lost,
        "optional iroh listener terminated unexpectedly"
    );
    Ok(())
}

enum RuntimeIo {
    Tcp(TcpStream),
    Iroh(QuicIo),
}

#[derive(Clone, Debug)]
pub(crate) struct TransportPeer {
    pub(crate) tcp: Option<SocketAddr>,
    pub(crate) iroh: Option<EndpointId>,
}

impl Connected for RuntimeIo {
    type ConnectInfo = TransportPeer;
    fn connect_info(&self) -> Self::ConnectInfo {
        let peer = match self {
            Self::Tcp(io) => TransportPeer {
                tcp: io.peer_addr().ok(),
                iroh: None,
            },
            Self::Iroh(io) => TransportPeer {
                tcp: None,
                iroh: Some(io.peer_id()),
            },
        };
        tracing::debug!(tcp = ?peer.tcp, iroh = ?peer.iroh, "gRPC transport peer");
        peer
    }
}

impl AsyncRead for RuntimeIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Tcp(io) => Pin::new(io).poll_read(cx, buf),
            Self::Iroh(io) => Pin::new(io).poll_read(cx, buf),
        }
    }
}
impl AsyncWrite for RuntimeIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            Self::Tcp(io) => Pin::new(io).poll_write(cx, buf),
            Self::Iroh(io) => Pin::new(io).poll_write(cx, buf),
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Tcp(io) => Pin::new(io).poll_flush(cx),
            Self::Iroh(io) => Pin::new(io).poll_flush(cx),
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Tcp(io) => Pin::new(io).poll_shutdown(cx),
            Self::Iroh(io) => Pin::new(io).poll_shutdown(cx),
        }
    }
}
