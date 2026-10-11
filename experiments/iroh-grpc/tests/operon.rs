//! Real daemon compatibility, via a PoC-only in-process TCP loopback bridge.
//! The native tonic incoming path is separately covered in transport.rs.
use anyhow::Result;
use operon_iroh_grpc_poc::{
    QuicIo, channel, local_endpoint,
    runtime::{operon_runtime_client::OperonRuntimeClient, *},
};
use std::{
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpStream, task::JoinSet};
use tonic::{Request, transport::Channel};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn request<T>(value: T) -> Request<T> {
    let mut request = Request::new(value);
    request
        .metadata_mut()
        .insert("authorization", "Bearer poc-only-token".parse().unwrap());
    request
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unchanged_operon_daemon_over_iroh() -> Result<()> {
    let daemon_bin = std::env::var("OPERON_POC_DAEMON").map_err(|_| {
        anyhow::anyhow!(
            "set OPERON_POC_DAEMON to an existing operond binary; this test must not silently skip"
        )
    })?;
    let timeout = std::env::var("OPERON_POC_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "60".into())
        .parse::<u64>()?;
    anyhow::ensure!(timeout > 0, "timeout must be positive");
    tokio::time::timeout(Duration::from_secs(timeout), async {
        let temp = tempfile::tempdir()?;
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace)?;
        // Release the reservation just before spawn; readiness checks the owned
        // child's status and authenticated node id, not mere port availability.
        let reserve = std::net::TcpListener::bind("127.0.0.1:0")?;
        let addr = reserve.local_addr()?;
        let config = temp.path().join("config.yaml");
        std::fs::write(
            &config,
            format!(
                r#"version: 1
daemon:
  node_id: iroh-poc-owned
  grpc_listen: {addr}
  workspace: {}
  advertise_lan: false
  store: {}
  auth:
    token: poc-only-token
policy:
  subject: poc
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: true
  exec:
    allowed_cwds: [/]
    default_timeout_secs: 30
    max_timeout_secs: 300
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
"#,
                workspace.display(),
                temp.path().join("store.jsonl").display()
            ),
        )?;
        let log = std::fs::File::create(temp.path().join("daemon.log"))?;
        drop(reserve);
        let mut daemon = Daemon(
            Command::new(&daemon_bin)
                .args(["start", "--config", config.to_str().unwrap()])
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()?,
        );
        let direct = loop {
            if let Some(status) = daemon.0.try_wait()? {
                anyhow::bail!(
                    "owned daemon exited {status}: {}",
                    std::fs::read_to_string(temp.path().join("daemon.log"))?
                );
            }
            if let Ok(channel) = Channel::from_shared(format!("http://{addr}"))?
                .connect()
                .await
            {
                let mut client = OperonRuntimeClient::new(channel.clone());
                if let Ok(health) = client.health(request(HealthRequest {})).await
                    && health.into_inner().node_id == "iroh-poc-owned"
                {
                    break channel;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let server = local_endpoint().await?;
        let client_endpoint = local_endpoint().await?;
        let expected_peer = client_endpoint.id();
        let accept_endpoint = server.clone();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let bridge_shutdown = shutting_down.clone();
        let bridge = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    incoming = accept_endpoint.accept() => {
                        let Some(incoming) = incoming else { break; };
                        let conn = incoming.await?;
                        anyhow::ensure!(conn.remote_id() == expected_peer, "peer not admitted");
                        let (send, recv) = conn.accept_bi().await?;
                        tasks.spawn(async move {
                            let mut io = QuicIo::new(conn, send, recv);
                            let mut tcp = TcpStream::connect(addr).await?;
                            // Cancellation ends the copy and drops the TCP side.
                            tokio::io::copy_bidirectional(&mut io, &mut tcp).await
                        });
                    }
                    // Individual copy tasks fail with connection errors during
                    // normal teardown; that is expected, not a bridge failure.
                    Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                        match result {
                            Ok(Ok(_)) => {},
                            Ok(Err(error)) if bridge_shutdown.load(Ordering::SeqCst) => {
                                eprintln!("owned bridge shutdown: {error}");
                            },
                            Ok(Err(error)) => return Err(error.into()),
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
            }
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            Ok::<_, anyhow::Error>(())
        });
        let quic = channel(
            client_endpoint.clone(),
            server.addr(),
            Duration::from_secs(10),
        )
        .await?;
        let mut client = OperonRuntimeClient::new(quic.clone())
            .max_decoding_message_size(16 * 1024 * 1024)
            .max_encoding_message_size(16 * 1024 * 1024);
        assert_eq!(
            client
                .health(request(HealthRequest {}))
                .await?
                .into_inner()
                .node_id,
            "iroh-poc-owned"
        );
        assert_eq!(
            client.health(HealthRequest {}).await.unwrap_err().code(),
            tonic::Code::Unauthenticated
        );
        println!("PASS real Operon health, bearer auth preserved over iroh");
        let payload: Vec<u8> = (0..8 * 1024 * 1024 + 31).map(|i| (i % 251) as u8).collect();
        let target = WriteFileRequest {
            payload: Some(write_file_request::Payload::Target(WriteFileTarget {
                path: "/payload.bin".into(),
                ..Default::default()
            })),
        };
        let frames = std::iter::once(target)
            .chain(payload.chunks(65536).map(|data| WriteFileRequest {
                payload: Some(write_file_request::Payload::Chunk(FileChunk {
                    data: data.to_vec(),
                })),
            }))
            .collect::<Vec<_>>();
        assert_eq!(
            client
                .write_file(request(tokio_stream::iter(frames)))
                .await?
                .into_inner()
                .bytes_written,
            payload.len() as u64
        );
        let mut stream = client
            .read_file(request(FsPathRequest {
                path: "/payload.bin".into(),
                ..Default::default()
            }))
            .await?
            .into_inner();
        let mut read = Vec::new();
        while let Some(chunk) = stream.message().await? {
            read.extend_from_slice(&chunk.data);
        }
        assert_eq!(read, payload);
        let range = client
            .read_file_range(request(FsReadRangeRequest {
                path: "/payload.bin".into(),
                offset: 123,
                size: 4 * 1024 * 1024,
            }))
            .await?
            .into_inner()
            .data;
        assert_eq!(range, payload[123..123 + 4 * 1024 * 1024]);
        assert_eq!(
            client
                .read_file(request(FsPathRequest {
                    path: "/../outside".into(),
                    ..Default::default()
                }))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
        println!("PASS actual fs streamed write/read >8MiB, >2MiB range, path rejection");
        let record = client
            .run_exec(request(ExecRunRequest {
                cwd: "/".into(),
                argv: vec!["sh".into(), "-c".into(), "printf iroh-poc-exec".into()],
                ..Default::default()
            }))
            .await?
            .into_inner();
        let mut watch = client
            .watch_exec(request(ExecIdRequest {
                exec_id: record.id.clone(),
            }))
            .await?
            .into_inner();
        let mut success = false;
        while let Some(event) = watch.message().await? {
            if let Some(code) = event.exit_code {
                assert_eq!(code, 0);
                success = true;
            }
        }
        assert!(success);
        let logs = client
            .list_exec_logs(request(ExecIdRequest { exec_id: record.id }))
            .await?
            .into_inner();
        let output = logs
            .logs
            .into_iter()
            .flat_map(|log| log.data)
            .collect::<Vec<_>>();
        assert_eq!(output, b"iroh-poc-exec");
        println!("PASS real exec, completion stream and binary log content");
        // Existing direct network path remains available alongside iroh.
        assert!(
            OperonRuntimeClient::new(direct)
                .health(request(HealthRequest {}))
                .await?
                .into_inner()
                .ok
        );
        println!("PASS direct TCP and iroh coexist without production changes");
        shutting_down.store(true, Ordering::SeqCst);
        drop(client);
        drop(quic);
        client_endpoint.close().await;
        server.close().await;
        bridge.await??;
        println!("PASS owned bridge tasks joined; owned daemon cleaned by RAII");
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}
