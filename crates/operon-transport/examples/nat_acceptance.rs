//! Test-only observer: real daemon gRPC on one connection across NAT path upgrade.
use operon_protocol::runtime::v1::{operon_runtime_client::OperonRuntimeClient, HealthRequest};
use operon_transport::embedded::{EmbeddedEndpoint, EndpointAddr, EndpointId, TransportAddr};
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: nat_acceptance <config> <peer-public-IP>".into());
    }
    let seconds: u64 = std::env::var("OPERON_EMBEDDED_TEST_TIMEOUT_SECS")?.parse()?;
    if seconds == 0 {
        return Err("acceptance timeout must be positive".into());
    }
    let timeout = Duration::from_secs(seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    let config_path = Path::new(&args[1]);
    let config = operon_config::OperonConfig::load(config_path)?;
    let peer = config.endpoint(
        "remote",
        &operon_config::OperonConfig::config_dir(config_path),
    )?;
    let settings = peer.transport.iroh.as_ref().ok_or("missing iroh config")?;
    let endpoint = tokio::time::timeout_at(
        deadline,
        EmbeddedEndpoint::from_config(&settings.local, Path::new("/")),
    )
    .await??;
    let result = tokio::time::timeout_at(deadline, async {
        eprintln!("owned endpoint bound; waiting for TLS relay readiness");
        endpoint.wait_online(timeout).await?;
        eprintln!("observed client addresses: {:?}", endpoint.addr());
        eprintln!("owned TLS relay ready; connecting pinned peer");
        let id: EndpointId = peer.endpoint.strip_prefix("iroh://").ok_or("missing iroh URI")?.parse()?;
        let relay = settings.peer_relay_urls.first().ok_or("missing peer relay")?.parse()?;
        let stream = endpoint.connect(EndpointAddr::from_parts(id, [TransportAddr::Relay(relay)]), timeout).await?;
        let connection = stream.connection().clone();
        eprintln!("pinned peer connected; opening real daemon HTTP/2");
        let once = Arc::new(Mutex::new(Some(stream)));
        let channel = tonic::transport::Endpoint::from_static("http://iroh.invalid")
            .connect_with_connector(tower::service_fn(move |_| {
                let stream = once.lock().unwrap().take();
                async move { stream.map(hyper_util::rt::TokioIo::new).ok_or_else(|| io::Error::other("unexpected reconnect/replay")) }
            })).await?;
        let mut client = OperonRuntimeClient::new(channel);
        let health_request = || -> Result<_, Box<dyn std::error::Error>> {
            let mut request = tonic::Request::new(HealthRequest {});
            request.metadata_mut().insert("authorization", format!("Bearer {}", peer.token.as_deref().ok_or("missing token")?).parse()?);
            Ok(request)
        };
        assert!(client.health(health_request()?).await?.into_inner().ok);
        assert!(connection.paths().iter().any(|path| path.is_selected() && path.is_relay()), "initial path must be relay while peer UDP is filtered");
        std::fs::write("/state/relay-ready", b"verified")?;
        while !Path::new("/state/allow-direct").exists() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let public_ip: std::net::IpAddr = args[2].parse()?;
        let mut paths = Box::pin(connection.paths_stream());
        let mut traffic = tokio::time::interval(Duration::from_secs(1));
        loop {
            let snapshot = tokio::select! {
                snapshot = paths.next() => snapshot.ok_or("connection closed before NAT upgrade")?,
                _ = traffic.tick() => {
                    assert!(client.health(health_request()?).await?.into_inner().ok);
                    continue;
                }
            };
            for path in snapshot.iter() { eprintln!("candidate selected={} remote={:?}", path.is_selected(), path.remote_addr()); }
            if snapshot.iter().any(|path| path.is_selected() && matches!(path.remote_addr(), TransportAddr::Ip(addr) if addr.ip() == public_ip)) { break; }
        }
        assert!(client.health(health_request()?).await?.into_inner().ok);
        std::fs::write("/state/direct-verified", b"verified")?;
        drop(paths);
        drop(client);
        connection.close(0u32.into(), b"NAT acceptance complete");
        println!("PASS real daemon gRPC: selected relay -> selected SNAT public IP on same connection");
        Ok::<_, Box<dyn std::error::Error>>(())
    }).await;
    // Even timeout/error paths must join endpoint teardown before the fixture exits.
    endpoint.close().await;
    result??;
    Ok(())
}
