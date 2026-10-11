#![cfg(feature = "iroh")]

use std::{io, time::Duration};

use operon_transport::embedded::{
    BindOptions, EmbeddedEndpoint, EndpointAddr, QuicTransportConfig, RelayUrl, SecretKey,
    TransportAddr,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamExt;

// Invoked explicitly by the isolated self-hosted relay acceptance gate.
// Ordinary unit tests must not require or accidentally contact public relays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires OPERON_TEST_RELAY_URL pointing at an owned local relay"]
async fn relay_bootstrap_upgrades_to_selected_direct_path() -> io::Result<()> {
    let relay: RelayUrl = std::env::var("OPERON_TEST_RELAY_URL")
        .map_err(io::Error::other)?
        .parse()
        .map_err(io::Error::other)?;
    let seconds = std::env::var("OPERON_EMBEDDED_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "120".into())
        .parse::<u64>()
        .map_err(io::Error::other)?;
    if seconds == 0 {
        return Err(io::Error::other("acceptance timeout must be positive"));
    }
    let deadline = Duration::from_secs(seconds);
    tokio::time::timeout(deadline, async {
        let options = |identity| BindOptions {
            identity: SecretKey::from_bytes(&[identity; 32]),
            bind_addrs: vec!["127.0.0.1:0".parse().unwrap()],
            relay_urls: vec![relay.clone()], bind_timeout: deadline,
            quic: QuicTransportConfig::builder()
                .max_concurrent_bidi_streams(1u32.into()).build(),
        };
        let server = EmbeddedEndpoint::bind(options(81)).await?;
        let client = EmbeddedEndpoint::bind(options(82)).await?;
        server.wait_online(deadline).await?;
        client.wait_online(deadline).await?;
        let mut incoming = server.incoming(2, deadline)?;
        // The connector starts with only a relay address, never a supplied IP.
        let target = EndpointAddr::from_parts(server.id(), [TransportAddr::Relay(relay)]);
        let receiving = async {
            let mut stream = incoming.next().await.ok_or_else(|| io::Error::other("listener ended"))??;
            assert_eq!(stream.peer_id(), client.id());
            let mut request = [0; 8];
            stream.read_exact(&mut request).await?;
            assert_eq!(&request, b"upgrade!");
            stream.write_all(&request).await?;
            stream.read_exact(&mut request).await?;
            assert_eq!(&request, b"direct!!");
            stream.write_all(&request).await?;
            stream.connection().closed().await;
            Ok::<_, io::Error>(())
        };
        let sending = async {
            let mut stream = client.connect(target, deadline).await?;
            assert_eq!(stream.peer_id(), server.id());
            stream.write_all(b"upgrade!").await?;
            let mut response = [0; 8];
            stream.read_exact(&mut response).await?;
            assert_eq!(&response, b"upgrade!");
            {
                let mut paths = Box::pin(stream.connection().paths_stream());
                loop {
                    let snapshot = paths.next().await.ok_or_else(|| io::Error::other("connection closed before direct upgrade"))?;
                    if snapshot.iter().any(|path| path.is_selected() && path.is_ip()) { break; }
                }
            }
            stream.write_all(b"direct!!").await?;
            stream.read_exact(&mut response).await?;
            assert_eq!(&response, b"direct!!");
            Ok::<_, io::Error>(())
        };
        let (receiving, sending) = tokio::join!(receiving, sending);
        receiving?;
        sending?;
        incoming.shutdown().await?;
        client.close().await;
        println!("PASS relay-only bootstrap addresses upgrade to selected direct path; this is not NAT evidence");
        Ok(())
    }).await.map_err(io::Error::other)?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires OPERON_TEST_RELAY_URL pointing at an owned local relay"]
async fn relay_only_stream_authentication_payload_and_owned_shutdown() -> io::Result<()> {
    let relay: RelayUrl = std::env::var("OPERON_TEST_RELAY_URL")
        .map_err(io::Error::other)?
        .parse()
        .map_err(io::Error::other)?;
    let seconds = std::env::var("OPERON_EMBEDDED_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "120".into())
        .parse::<u64>()
        .map_err(io::Error::other)?;
    if seconds == 0 {
        return Err(io::Error::other("acceptance timeout must be positive"));
    }
    let deadline = Duration::from_secs(seconds);
    tokio::time::timeout(deadline, async {
        let options = |identity| BindOptions {
            identity: SecretKey::from_bytes(&[identity; 32]),
            // No IP transports: this test cannot silently use direct UDP.
            bind_addrs: vec![],
            relay_urls: vec![relay.clone()],
            bind_timeout: deadline,
            quic: QuicTransportConfig::builder()
                .max_concurrent_bidi_streams(1u32.into())
                .max_concurrent_uni_streams(0u32.into())
                .build(),
        };
        let server = EmbeddedEndpoint::bind(options(71)).await?;
        let client = EmbeddedEndpoint::bind(options(72)).await?;
        server.wait_online(deadline).await?;
        client.wait_online(deadline).await?;
        let mut incoming = server.incoming(2, deadline)?;
        let payload: Vec<u8> = (0..8 * 1024 * 1024 + 117).map(|n| (n % 251) as u8).collect();
        let target = EndpointAddr::from_parts(server.id(), [TransportAddr::Relay(relay)]);
        let receiving = async {
            let mut stream = incoming.next().await.ok_or_else(|| io::Error::other("listener ended"))??;
            assert_eq!(stream.peer_id(), client.id());
            let mut received = vec![0; payload.len()];
            stream.read_exact(&mut received).await?;
            assert_eq!(received, payload);
            assert!(stream.connection().paths().iter().any(|path| path.is_relay()));
            stream.write_all(b"verified").await?;
            stream.connection().closed().await;
            Ok::<_, io::Error>(())
        };
        let sending = async {
            let mut stream = client.connect(target, deadline).await?;
            assert_eq!(stream.peer_id(), server.id());
            stream.write_all(&payload).await?;
            let mut response = [0; 8];
            stream.read_exact(&mut response).await?;
            assert_eq!(&response, b"verified");
            assert!(stream.connection().paths().iter().any(|path| path.is_relay()));
            drop(stream);
            Ok::<_, io::Error>(())
        };
        let (receiving, sending) = tokio::join!(receiving, sending);
        receiving?;
        sending?;
        incoming.shutdown().await?;
        client.close().await;
        println!("PASS owned self-hosted relay path, pinned peer identities, >8MiB payload and joined shutdown");
        Ok(())
    }).await.map_err(io::Error::other)?
}
