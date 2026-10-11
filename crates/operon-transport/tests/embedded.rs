#![cfg(feature = "iroh")]

use std::{io, sync::Arc, time::Duration};

use operon_transport::embedded::{
    BindOptions, EmbeddedEndpoint, EndpointAddr, QuicTransportConfig, SecretKey,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamExt;
use tonic::transport::{server::Connected, Endpoint, Server};
use tonic_health::pb::{health_client::HealthClient, HealthCheckRequest};

fn options(identity: u8) -> BindOptions {
    BindOptions {
        identity: SecretKey::from_bytes(&[identity; 32]),
        bind_addrs: vec!["127.0.0.1:0".parse().unwrap()],
        relay_urls: vec![],
        bind_timeout: Duration::from_secs(10),
        quic: QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(1u32.into())
            .max_concurrent_uni_streams(0u32.into())
            .build(),
    }
}

async fn bounded(future: impl std::future::Future<Output = io::Result<()>>) -> io::Result<()> {
    let seconds = std::env::var("OPERON_EMBEDDED_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "60".into())
        .parse::<u64>()
        .map_err(io::Error::other)?;
    if seconds == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "test deadline must be positive",
        ));
    }
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .map_err(io::Error::other)?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticated_stream_preserves_large_payload_and_half_close() -> io::Result<()> {
    bounded(async {
        let server = EmbeddedEndpoint::bind(options(1)).await?;
        let client = EmbeddedEndpoint::bind(options(2)).await?;
        let mut incoming = server.incoming(2, Duration::from_secs(10))?;
        let expected_peer = client.id();
        let payload: Vec<_> = (0..(8 * 1024 * 1024 + 117))
            .map(|n| (n % 251) as u8)
            .collect();
        let server_payload = payload.clone();
        let handler = async {
            let mut io = incoming
                .next()
                .await
                .ok_or_else(|| io::Error::other("incoming ended"))??;
            assert_eq!(io.connect_info(), expected_peer);
            let mut received = Vec::new();
            io.read_to_end(&mut received).await?;
            assert_eq!(received, server_payload);
            io.write_all(b"verified").await?;
            // Hold the connection until the receiver acknowledges delivery.
            io.connection().closed().await;
            Ok::<_, io::Error>(())
        };
        let requester = async {
            let mut io = client
                .connect(server.addr(), Duration::from_secs(10))
                .await?;
            assert_eq!(io.peer_id(), server.id());
            io.write_all(&payload).await?;
            io.shutdown().await?;
            let mut response = [0u8; 8];
            io.read_exact(&mut response).await?;
            assert_eq!(&response, b"verified");
            drop(io);
            Ok::<_, io::Error>(())
        };
        let (handler, requester) = tokio::join!(handler, requester);
        handler?;
        requester?;
        incoming.shutdown().await?;
        client.close().await;
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_tonic_connector_and_incoming_have_owned_shutdown() -> io::Result<()> {
    bounded(async {
        let server = EmbeddedEndpoint::bind(options(3)).await?;
        let client_endpoint = Arc::new(EmbeddedEndpoint::bind(options(4)).await?);
        let target = server.addr();
        let (reporter, service) = tonic_health::server::health_reporter();
        reporter
            .set_service_status("", tonic_health::ServingStatus::Serving)
            .await;
        let mut incoming = server.incoming(8, Duration::from_secs(10))?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let serving = tokio::spawn(async move {
            let result = Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(&mut incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await;
            incoming.shutdown().await?;
            result.map_err(io::Error::other)
        });
        let connector_endpoint = client_endpoint.clone();
        let connector = tower::service_fn(move |_| {
            let endpoint = connector_endpoint.clone();
            let target = target.clone();
            async move {
                endpoint
                    .connect(target, Duration::from_secs(10))
                    .await
                    .map(hyper_util::rt::TokioIo::new)
            }
        });
        let channel = Endpoint::from_static("http://iroh.invalid")
            .connect_with_connector(connector)
            .await
            .map_err(io::Error::other)?;
        let mut health = HealthClient::new(channel);
        for _ in 0..32 {
            let response = health
                .check(HealthCheckRequest {
                    service: String::new(),
                })
                .await
                .map_err(io::Error::other)?;
            assert_eq!(response.into_inner().status, 1);
        }
        drop(health);
        client_endpoint.close().await;
        shutdown_tx
            .send(())
            .map_err(|_| io::Error::other("server already stopped"))?;
        serving.await.map_err(io::Error::other)??;
        server.close().await;
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zero_deadline_stalled_stream_admission_is_cancelled_and_joined() -> io::Result<()> {
    bounded(async {
        let server = EmbeddedEndpoint::bind(options(5)).await?;
        let client = EmbeddedEndpoint::bind(options(6)).await?;
        let mut incoming = server.incoming(1, Duration::ZERO)?;
        // open_bi without writing leaves the server waiting for stream bytes.
        let stalled = client
            .connect(server.addr(), Duration::from_secs(10))
            .await?;
        tokio::time::timeout(Duration::from_secs(10), incoming.shutdown())
            .await
            .map_err(io::Error::other)??;
        drop(stalled);
        client.close().await;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn configuration_is_explicit_and_identity_is_stable() -> io::Result<()> {
    let mut disabled = options(7);
    disabled.bind_addrs.clear();
    assert_eq!(
        EmbeddedEndpoint::bind(disabled).await.err().unwrap().kind(),
        io::ErrorKind::InvalidInput
    );
    let first = EmbeddedEndpoint::bind(options(7)).await?;
    assert!(first.incoming(0, Duration::ZERO).is_err());
    assert!(first.incoming(4097, Duration::ZERO).is_err());
    let id = first.id();
    assert!(first.addr().relay_urls().next().is_none());
    first.close().await;
    let second = EmbeddedEndpoint::bind(options(7)).await?;
    assert_eq!(second.id(), id);
    second.close().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn active_connection_limit_refuses_excess_peer_and_recovers_after_drop() -> io::Result<()> {
    bounded(async {
        let server = EmbeddedEndpoint::bind(options(10)).await?;
        let first = EmbeddedEndpoint::bind(options(11)).await?;
        let second = EmbeddedEndpoint::bind(options(12)).await?;
        let mut incoming = server.incoming(1, Duration::from_secs(10))?;
        let mut connection = first
            .connect(server.addr(), Duration::from_secs(10))
            .await?;
        connection.write_all(b"hello").await?;
        let accepted = incoming.next().await.unwrap()?;
        assert!(second
            .connect(server.addr(), Duration::from_secs(1))
            .await
            .is_err());
        drop(accepted);
        drop(connection);
        let mut next = second
            .connect(server.addr(), Duration::from_secs(10))
            .await?;
        next.write_all(b"hello").await?;
        let accepted = incoming.next().await.unwrap()?;
        assert_eq!(accepted.peer_id(), second.id());
        drop(accepted);
        drop(next);
        incoming.shutdown().await?;
        first.close().await;
        second.close().await;
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_pinned_identity_never_authenticates() -> io::Result<()> {
    bounded(async {
        let server = EmbeddedEndpoint::bind(options(20)).await?;
        let client = EmbeddedEndpoint::bind(options(21)).await?;
        let mut incoming = server.incoming(2, Duration::from_secs(1))?;
        let addr = server.addr();
        let wrong_id = SecretKey::from_bytes(&[22; 32]).public();
        let wrong = EndpointAddr::from_parts(wrong_id, addr.addrs.clone());
        assert!(client.connect(wrong, Duration::from_secs(1)).await.is_err());
        incoming.shutdown().await?;
        client.close().await;
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relay_readiness_zero_deadline_is_cancelled_by_endpoint_close() -> io::Result<()> {
    bounded(async {
        let direct = EmbeddedEndpoint::bind(options(30)).await?;
        assert_eq!(
            direct.wait_online(Duration::ZERO).await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        direct.close().await;
        let mut relay = options(31);
        relay.bind_addrs.clear();
        relay.relay_urls = vec!["http://127.0.0.1:9".parse().unwrap()];
        let endpoint = Arc::new(EmbeddedEndpoint::bind(relay).await?);
        assert_eq!(
            endpoint
                .wait_online(Duration::from_millis(10))
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        let waiting_endpoint = endpoint.clone();
        let waiting =
            tokio::spawn(async move { waiting_endpoint.wait_online(Duration::ZERO).await });
        endpoint.close().await;
        assert_eq!(
            waiting.await.map_err(io::Error::other)?.unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        Ok(())
    })
    .await
}
