//! Actual native daemon startup and shared Rust client: no loopback bridge.
use anyhow::Result;
use operon_core::{IrohConnectionConfig, IrohEndpointConfig, NodeEndpoint, TransportConfig};
use operon_grpc_client::{connect_channel, request, runtime_client};
use operon_protocol::runtime::v1::*;
use std::{
    net::{SocketAddr, TcpListener, UdpSocket},
    time::Duration,
};

struct Running {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
}
impl Running {
    fn start(path: std::path::PathBuf) -> Self {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(crate::start_with_shutdown(
            crate::StartArgs {
                config: Some(path),
                shutdown_timeout_secs: 0,
            },
            async {
                let _ = rx.await;
            },
        ));
        Self {
            shutdown: Some(tx),
            task: Some(task),
        }
    }
    async fn stop(mut self) -> Result<()> {
        self.shutdown
            .take()
            .unwrap()
            .send(())
            .map_err(|_| anyhow::anyhow!("daemon already stopped"))?;
        self.task.take().unwrap().await??;
        Ok(())
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn leases() -> Result<(TcpListener, UdpSocket)> {
    // Outside hosted platform default outbound ephemeral ranges; retain both
    // reservations until immediately before native startup.
    for port in 20000..30000 {
        if let Ok(tcp) = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)) {
            if let Ok(udp) = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port)) {
                return Ok((tcp, udp));
            }
        }
    }
    anyhow::bail!("no native daemon fixture port available")
}

async fn bounded(future: impl std::future::Future<Output = Result<()>>) -> Result<()> {
    let seconds: u64 = std::env::var("OPERON_EMBEDDED_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "120".into())
        .parse()?;
    anyhow::ensure!(seconds > 0, "test deadline must be positive");
    tokio::time::timeout(Duration::from_secs(seconds), future).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_daemon_iroh_auth_fs_exec_and_tcp_coexist() -> Result<()> {
    bounded(async {
        let dir = tempfile::tempdir()?;
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace)?;
        let identity = operon_transport::identity::load_or_create_identity(&dir.path().join("server.key"))?.public();
        let (tcp_lease, udp_lease) = leases()?;
        let tcp_addr = tcp_lease.local_addr()?;
        let udp_addr: SocketAddr = udp_lease.local_addr()?;
        let config = dir.path().join("config.yaml");
        std::fs::write(&config, format!(r#"version: 1
daemon:
  node_id: native-owned
  grpc_listen: {tcp_addr}
  workspace: {}
  auth:
    token: native-test-only-token
  iroh:
    identity_file: server.key
    bind_addrs: [{udp_addr}]
    handshake_timeout_secs: 0
policy:
  subject: native-test
  fs:
    mounts:
      - name: workspace
        path: /
        permissions: {{read: true, write: true, delete: true}}
  exec:
    allowed_cwds: [/]
    default_timeout_secs: 30
    max_timeout_secs: 300
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
"#, workspace.display()))?;
        drop(tcp_lease);
        drop(udp_lease);
        let mut running = Running::start(config);
        let endpoint = NodeEndpoint {
            node_id: "native-owned".into(), endpoint: format!("iroh://{identity}"),
            token: Some("native-test-only-token".into()),
            transport: TransportConfig {
                iroh: Some(IrohConnectionConfig {
                    local: IrohEndpointConfig { identity_file: dir.path().join("client.key"),
                        bind_addrs: vec!["127.0.0.1:0".parse()?], ..Default::default() },
                    peer_addrs: vec![udp_addr], peer_relay_urls: vec![],
                }), ..Default::default()
            },
        };
        let channel = loop {
            if running.task.as_ref().unwrap().is_finished() {
                running.task.take().unwrap().await??;
                anyhow::bail!("native daemon ended before readiness");
            }
            if let Ok(channel) = connect_channel(&endpoint, Duration::from_millis(250)).await {
                let mut client = runtime_client(channel.clone());
                if let Ok(health) = client.health(request(&endpoint, HealthRequest {})?).await {
                    if health.into_inner().node_id == "native-owned" { break channel; }
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let mut client = runtime_client(channel);
        assert_eq!(client.health(HealthRequest {}).await.unwrap_err().code(), tonic::Code::Unauthenticated);
        let payload: Vec<u8> = (0..8 * 1024 * 1024 + 31).map(|i| (i % 251) as u8).collect();
        let frames = std::iter::once(WriteFileRequest { payload: Some(write_file_request::Payload::Target(
            WriteFileTarget { path: "/payload.bin".into(), ..Default::default() })) })
            .chain(payload.chunks(65536).map(|data| WriteFileRequest {
                payload: Some(write_file_request::Payload::Chunk(FileChunk { data: data.to_vec() }))
            })).collect::<Vec<_>>();
        assert_eq!(client.write_file(request(&endpoint, tokio_stream::iter(frames))?).await?.into_inner().bytes_written,
            payload.len() as u64);
        let mut stream = client.read_file(request(&endpoint, FsPathRequest { path: "/payload.bin".into(), ..Default::default() })?)
            .await?.into_inner();
        let mut received = Vec::new();
        while let Some(chunk) = stream.message().await? { received.extend_from_slice(&chunk.data); }
        assert_eq!(received, payload);
        let range = client.read_file_range(request(&endpoint, FsReadRangeRequest {
            path: "/payload.bin".into(), offset: 123, size: 4 * 1024 * 1024,
        })?).await?.into_inner().data;
        assert_eq!(range, payload[123..123 + 4 * 1024 * 1024]);
        assert_eq!(client.read_file(request(&endpoint, FsPathRequest { path: "/../outside".into(), ..Default::default() })?)
            .await.unwrap_err().code(), tonic::Code::PermissionDenied);
        let argv = if cfg!(windows) { vec!["cmd", "/C", "echo native-iroh"] }
            else { vec!["sh", "-c", "printf native-iroh"] };
        let record = client.run_exec(request(&endpoint, ExecRunRequest {
            cwd: "/".into(), argv: argv.into_iter().map(String::from).collect(), ..Default::default()
        })?).await?.into_inner();
        let mut watch = client.watch_exec(request(&endpoint, ExecIdRequest { exec_id: record.id.clone() })?)
            .await?.into_inner();
        let mut success = false;
        while let Some(event) = watch.message().await? {
            if let Some(code) = event.exit_code { assert_eq!(code, 0); success = true; }
        }
        assert!(success);
        let logs = client.list_exec_logs(request(&endpoint, ExecIdRequest { exec_id: record.id })?).await?.into_inner();
        let output: Vec<_> = logs.logs.into_iter().flat_map(|log| log.data).collect();
        assert_eq!(std::str::from_utf8(&output)?.trim(), "native-iroh");
        let tcp_endpoint = NodeEndpoint { endpoint: format!("grpc://{tcp_addr}"), transport: Default::default(), ..endpoint.clone() };
        let mut direct = operon_grpc_client::connect(&tcp_endpoint).await?;
        assert!(direct.health(request(&tcp_endpoint, HealthRequest {})?).await?.into_inner().ok);
        drop(direct);
        drop(client);
        running.stop().await?;
        assert_eq!(operon_transport::identity::load_identity(&dir.path().join("server.key"))?.public(), identity);
        println!("PASS native daemon/client iroh auth, >8MiB fs, 4MiB range, traversal denial, exec, mixed TCP and zero-deadline joined shutdown");
        Ok(())
    }).await
}

#[tokio::test]
async fn embedded_listener_rejects_missing_auth_before_creating_identity() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("config.yaml");
    std::fs::write(&config, format!("version: 1\ndaemon:\n  node_id: denied\n  grpc_listen: 127.0.0.1:0\n  workspace: {}\n  iroh:\n    identity_file: not-created.key\n", dir.path().display()))?;
    let error = crate::start_with_shutdown(
        crate::StartArgs {
            config: Some(config),
            shutdown_timeout_secs: 0,
        },
        std::future::pending(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("auth-token"));
    assert!(!dir.path().join("not-created.key").exists());
    Ok(())
}
