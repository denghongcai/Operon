//! Embedded iroh streams for native HTTP/2 gRPC incoming/connectors.
//!
//! One bidirectional stream carries one HTTP/2 connection. No TCP bridge,
//! public infrastructure defaults, address publication or RPC replay.

use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

pub use iroh::endpoint::QuicTransportConfig;
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    Endpoint, RelayMap, RelayMode,
};
pub use iroh::{EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    task::{JoinHandle, JoinSet},
};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::transport::server::Connected;

pub const ALPN: &[u8] = b"operon/grpc-h2/1";

fn load_relay_ca(path: &std::path::Path) -> io::Result<iroh_relay::tls::CaTlsConfig> {
    use rustls_pki_types::{pem::PemObject, CertificateDer};
    use std::io::Read;
    const LIMIT: usize = 1024 * 1024;
    let mut pem = Vec::new();
    std::fs::File::open(path)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut pem)?;
    if pem.len() > LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relay CA file exceeds 1 MiB",
        ));
    }
    let roots = CertificateDer::pem_slice_iter(&pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid relay CA certificate PEM",
            )
        })?;
    if roots.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relay CA file has no certificates",
        ));
    }
    Ok(iroh_relay::tls::CaTlsConfig::embedded().with_extra_roots(roots))
}

/// All connection infrastructure is explicit. Empty relays never select
/// public relays; an empty bind list disables direct IP transports.
pub struct BindOptions {
    pub identity: SecretKey,
    pub bind_addrs: Vec<SocketAddr>,
    pub relay_urls: Vec<RelayUrl>,
    pub ca_tls_config: Option<iroh_relay::tls::CaTlsConfig>,
    pub bind_timeout: Duration,
    /// Caller-selected idle, keepalive, path and stream/window limits.
    pub quic: QuicTransportConfig,
}

/// An application-owned endpoint. Call `close` during normal shutdown.
pub struct EmbeddedEndpoint {
    endpoint: Endpoint,
    has_relays: bool,
}

impl EmbeddedEndpoint {
    /// Resolve the private key relative to the config directory. This does not
    /// publish addresses or wait for public services; relay readiness is a
    /// separate, explicitly cancellable operation.
    pub async fn from_config(
        config: &operon_config::IrohEndpointConfig,
        config_dir: &std::path::Path,
    ) -> io::Result<Self> {
        config
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let relay_urls = parse_relays(&config.relay_urls, config.allow_insecure_relays)?;
        let ca_tls_config = config
            .relay_ca_file
            .as_ref()
            .map(|path| load_relay_ca(&operon_config::resolve_path(config_dir, path)))
            .transpose()?;
        let path = operon_config::resolve_path(config_dir, &config.identity_file);
        let identity = crate::identity::load_or_create_identity(&path)?;
        let idle = if config.idle_timeout_secs == 0 {
            None
        } else {
            Some(
                Duration::from_secs(config.idle_timeout_secs)
                    .try_into()
                    .map_err(io::Error::other)?,
            )
        };
        let quic = QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(1u32.into())
            .max_concurrent_uni_streams(0u32.into())
            .max_idle_timeout(idle)
            .keep_alive_interval(Duration::from_secs(config.keepalive_interval_secs))
            .default_path_max_idle_timeout(Duration::from_secs(config.path_idle_timeout_secs))
            .default_path_keep_alive_interval(Duration::from_secs(
                config.path_keepalive_interval_secs,
            ))
            .build();
        Self::bind(BindOptions {
            identity,
            bind_addrs: config.bind_addrs.clone(),
            relay_urls,
            ca_tls_config,
            bind_timeout: Duration::from_secs(config.bind_timeout_secs),
            quic,
        })
        .await
    }
    pub async fn bind(options: BindOptions) -> io::Result<Self> {
        if options.bind_addrs.is_empty() && options.relay_urls.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "iroh needs an explicit IP bind or relay",
            ));
        }
        let mut builder = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_relay_transports()
            .secret_key(options.identity)
            .transport_config(options.quic)
            .alpns(vec![ALPN.to_vec()]);
        if let Some(ca) = options.ca_tls_config {
            builder = builder.ca_tls_config(ca);
        }
        for addr in options.bind_addrs {
            builder = builder.bind_addr(addr).map_err(io::Error::other)?;
        }
        let has_relays = !options.relay_urls.is_empty();
        if has_relays {
            builder =
                builder.relay_mode(RelayMode::Custom(RelayMap::from_iter(options.relay_urls)));
        }
        let endpoint = deadline(options.bind_timeout, async {
            builder.bind().await.map_err(io::Error::other)
        })
        .await?;
        Ok(Self {
            endpoint,
            has_relays,
        })
    }

    pub fn id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    pub async fn close(&self) {
        self.endpoint.close().await;
    }

    /// Wait for explicitly configured relay connectivity. No implicit lookup
    /// service is enrolled; caller selects the readiness deadline.
    pub async fn wait_online(&self, timeout: Duration) -> io::Result<()> {
        if !self.has_relays {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no relay configured",
            ));
        }
        self.cancellable(timeout, async {
            self.endpoint.online().await;
            Ok(())
        })
        .await
    }

    /// Authenticate the pinned endpoint ID and open a single HTTP/2 stream.
    /// The returned I/O retains the endpoint/connection for its full lifetime.
    pub async fn connect(&self, target: EndpointAddr, timeout: Duration) -> io::Result<QuicIo> {
        self.cancellable(timeout, async {
            let connection = self
                .endpoint
                .connect(target, ALPN)
                .await
                .map_err(io::Error::other)?;
            let (send, recv) = connection.open_bi().await.map_err(io::Error::other)?;
            Ok(QuicIo::new(
                self.endpoint.clone(),
                connection,
                send,
                recv,
                None,
            ))
        })
        .await
    }

    async fn cancellable<T>(
        &self,
        timeout: Duration,
        future: impl Future<Output = io::Result<T>>,
    ) -> io::Result<T> {
        tokio::select! {
            biased;
            _ = self.endpoint.closed() => Err(io::Error::new(io::ErrorKind::NotConnected, "iroh endpoint closed")),
            result = deadline(timeout, future) => result,
        }
    }

    /// Bound pending handshakes, queued and active HTTP/2 connections together.
    /// Untrusted stalled peers cannot grow the task or incoming queue count.
    pub fn incoming(
        &self,
        max_connections: usize,
        handshake_timeout: Duration,
    ) -> io::Result<Incoming> {
        if !(1..=4096).contains(&max_connections) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "iroh max_connections must be 1..=4096",
            ));
        }
        let (tx, rx) = mpsc::channel(max_connections);
        let endpoint = self.endpoint.clone();
        let accept_endpoint = endpoint.clone();
        let task = tokio::spawn(async move {
            let slots = Arc::new(Semaphore::new(max_connections));
            let mut tasks = JoinSet::new();
            let mut result = loop {
                tokio::select! {
                    _ = tx.closed() => break Ok(()),
                    incoming = accept_endpoint.accept() => {
                        let Some(incoming) = incoming else { break Ok(()); };
                        let Ok(slot) = slots.clone().try_acquire_owned() else {
                            incoming.refuse();
                            continue;
                        };
                        let tx = tx.clone();
                        let endpoint = accept_endpoint.clone();
                        tasks.spawn(async move {
                            let io = deadline(handshake_timeout, async {
                                let connection = incoming.await.map_err(io::Error::other)?;
                                let (send, recv) = connection.accept_bi().await.map_err(io::Error::other)?;
                                Ok(QuicIo::new(endpoint, connection, send, recv, Some(slot)))
                            }).await?;
                            tx.send(Ok(io)).await.map_err(io::Error::other)
                        });
                    }
                    Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                        match result {
                            Ok(Ok(())) => {},
                            Ok(Err(error)) => tracing::debug!(%error, "iroh admission rejected"),
                            Err(error) => break Err(io::Error::other(error)),
                        }
                    }
                }
            };
            tasks.abort_all();
            while let Some(joined) = tasks.join_next().await {
                if let Err(error) = joined {
                    if !error.is_cancelled() {
                        result = Err(io::Error::other(error));
                    }
                }
            }
            result
        });
        Ok(Incoming {
            endpoint,
            receiver: ReceiverStream::new(rx),
            task: Some(task),
        })
    }
}

/// Target endpoint identity is independent of Operon's logical node_id.
pub fn target_address(
    endpoint: &str,
    config: &operon_config::IrohConnectionConfig,
) -> io::Result<EndpointAddr> {
    config
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let id = endpoint
        .strip_prefix("iroh://")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "expected iroh:// endpoint"))?
        .parse::<EndpointId>()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid iroh endpoint public key",
            )
        })?;
    if !config.peer_relay_urls.is_empty() && config.local.relay_urls.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "peer relay addressing requires local relay_urls to enable relay transport",
        ));
    }
    let addrs = config
        .peer_addrs
        .iter()
        .copied()
        .map(TransportAddr::Ip)
        .chain(
            parse_relays(&config.peer_relay_urls, config.local.allow_insecure_relays)?
                .into_iter()
                .map(TransportAddr::Relay),
        );
    Ok(EndpointAddr::from_parts(id, addrs))
}

fn parse_relays(urls: &[String], allow_insecure: bool) -> io::Result<Vec<RelayUrl>> {
    urls.iter().map(|url| {
        let url: RelayUrl = url.parse().map_err(io::Error::other)?;
        if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some()
            || !(url.scheme() == "https" || (allow_insecure && url.scheme() == "http")) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "iroh relay requires explicit HTTP(S) origin without credentials, query or fragment"));
        }
        Ok(url)
    }).collect()
}

async fn deadline<T>(
    timeout: Duration,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    if timeout.is_zero() {
        return future.await;
    }
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "iroh connection deadline exceeded"))?
}

/// A listener owns its admission task, with explicit close/abort/join shutdown.
/// Drop is a cancellation fallback; applications must call `shutdown` to
/// observe failures and establish that admission tasks have been joined.
pub struct Incoming {
    endpoint: Endpoint,
    receiver: ReceiverStream<io::Result<QuicIo>>,
    task: Option<JoinHandle<io::Result<()>>>,
}

impl Incoming {
    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.receiver.close();
        self.endpoint.close().await;
        if let Some(task) = self.task.take() {
            task.await.map_err(io::Error::other)??;
        }
        Ok(())
    }
}

impl Stream for Incoming {
    type Item = io::Result<QuicIo>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

impl Drop for Incoming {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

pub struct QuicIo {
    _endpoint: Endpoint,
    connection: Connection,
    send: SendStream,
    recv: RecvStream,
    _slot: Option<OwnedSemaphorePermit>,
}

impl QuicIo {
    fn new(
        endpoint: Endpoint,
        connection: Connection,
        send: SendStream,
        recv: RecvStream,
        slot: Option<OwnedSemaphorePermit>,
    ) -> Self {
        Self {
            _endpoint: endpoint,
            connection,
            send,
            recv,
            _slot: slot,
        }
    }

    pub fn peer_id(&self) -> EndpointId {
        self.connection.remote_id()
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

impl Connected for QuicIo {
    type ConnectInfo = EndpointId;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.peer_id()
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

impl Drop for QuicIo {
    fn drop(&mut self) {
        self.connection
            .close(0u32.into(), b"HTTP/2 transport dropped");
    }
}

#[cfg(test)]
mod relay_ca_tests {
    use super::*;

    #[test]
    fn rejects_missing_empty_malformed_and_oversized_trust_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ca.pem");
        assert!(load_relay_ca(&path).is_err());
        for data in [
            Vec::new(),
            b"-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----".to_vec(),
            vec![b'x'; 1024 * 1024 + 1],
        ] {
            std::fs::write(&path, data).unwrap();
            assert!(load_relay_ca(&path).is_err());
        }
    }
}
