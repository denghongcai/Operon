//! Real TCP fixtures for CLI pagination transport, auth/context and errors.
use futures_util::future::BoxFuture;
use operon_protocol::runtime::v1::*;
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tonic::{
    codegen::{http, Service},
    Request, Response, Status,
};

struct StalledUnaryBody(bool);
impl http_body::Body for StalledUnaryBody {
    type Data = <tonic::body::Body as http_body::Body>::Data;
    type Error = Status;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.0 {
            return Poll::Pending;
        }
        self.0 = true;
        // Promise 32 bytes of protobuf but never complete the message.
        Poll::Ready(Some(Ok(http_body::Frame::data(
            (&b"\0\0\0\0\x20"[..]).into(),
        ))))
    }
}

#[derive(Clone, Default)]
struct Pages {
    calls: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
    delay_ms: Arc<AtomicUsize>,
}
impl Pages {
    fn page<T>(&self, request: &Request<T>, token: &str) -> Result<(usize, String), Status> {
        for (key, expected) in [
            ("authorization", "Bearer test-token"),
            ("x-operon-run-id", "run-pages"),
            ("x-operon-step-id", "step-pages"),
        ] {
            assert_eq!(
                request.metadata().get(key).unwrap().to_str().unwrap(),
                expected
            );
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        let page = match token {
            "" => 0,
            "1" => 1,
            "2" => 2,
            _ => panic!("unexpected token"),
        };
        if self.fail.load(Ordering::SeqCst) && page == 2 {
            return Err(Status::unavailable("page failure"));
        }
        Ok((
            page,
            if page < 2 {
                (page + 1).to_string()
            } else {
                String::new()
            },
        ))
    }
}
impl tonic::server::UnaryService<FsListRequest> for Pages {
    type Response = FsList;
    type Future = BoxFuture<'static, Result<Response<FsList>, Status>>;
    fn call(&mut self, request: Request<FsListRequest>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            assert_eq!(request.get_ref().path, "/pages");
            assert_eq!(request.get_ref().page_size, 1000);
            let (page, next_page_token) = this.page(&request, &request.get_ref().page_token)?;
            tokio::time::sleep(std::time::Duration::from_millis(
                this.delay_ms.load(Ordering::SeqCst) as u64,
            ))
            .await;
            Ok(Response::new(FsList {
                path: "/pages".into(),
                entries: vec![FsEntry {
                    name: format!("file-{page}"),
                    path: format!("/pages/file-{page}"),
                    is_file: true,
                    ..Default::default()
                }],
                next_page_token,
            }))
        })
    }
}
impl tonic::server::UnaryService<ListCapabilitiesRequest> for Pages {
    type Response = CapabilityList;
    type Future = BoxFuture<'static, Result<Response<CapabilityList>, Status>>;
    fn call(&mut self, request: Request<ListCapabilitiesRequest>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            assert_eq!(request.get_ref().page_size, 1000);
            let (page, next_page_token) = this.page(&request, &request.get_ref().page_token)?;
            Ok(Response::new(CapabilityList {
                capabilities: vec![Capability {
                    id: format!("fs:{page}"),
                    kind: CapabilityKind::Fs as i32,
                    ..Default::default()
                }],
                next_page_token,
            }))
        })
    }
}
impl tonic::server::NamedService for Pages {
    const NAME: &'static str = "operon.runtime.v1.OperonRuntime";
}
impl tonic::server::ServerStreamingService<FsPathRequest> for Pages {
    type Response = FileChunk;
    type ResponseStream =
        std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<FileChunk, Status>> + Send>>;
    type Future = BoxFuture<'static, Result<Response<Self::ResponseStream>, Status>>;
    fn call(&mut self, request: Request<FsPathRequest>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            this.page(&request, "")?;
            let path = request.into_inner().path;
            if path == "/stall-headers" {
                return std::future::pending().await;
            }
            let stream = async_stream::stream! {
                if path == "/stall-body" {
                    yield Ok(FileChunk { data: vec![1] });
                    std::future::pending::<()>().await;
                } else {
                    for value in 0..12 {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        yield Ok(FileChunk { data: vec![value] });
                    }
                }
            };
            Ok(Response::new(Box::pin(stream) as Self::ResponseStream))
        })
    }
}
impl tonic::server::ClientStreamingService<WriteFileRequest> for Pages {
    type Response = FsWrite;
    type Future = BoxFuture<'static, Result<Response<FsWrite>, Status>>;
    fn call(&mut self, request: Request<tonic::Streaming<WriteFileRequest>>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            this.page(&request, "")?;
            let mut stream = request.into_inner();
            while stream.message().await?.is_some() {}
            std::future::pending().await
        })
    }
}
impl Service<http::Request<tonic::body::Body>> for Pages {
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: http::Request<tonic::body::Body>) -> Self::Future {
        let handler = self.clone();
        let path = request.uri().path().to_string();
        Box::pin(async move {
            if path.ends_with("/Health") {
                assert_eq!(request.headers()["authorization"], "Bearer test-token");
                Ok(http::Response::builder()
                    .header("content-type", "application/grpc")
                    .body(tonic::body::Body::new(StalledUnaryBody(false)))
                    .unwrap())
            } else if path.ends_with("/ReadFile") {
                Ok(tonic::server::Grpc::new(
                    tonic_prost::ProstCodec::<FileChunk, FsPathRequest>::default(),
                )
                .server_streaming(handler, request)
                .await)
            } else if path.ends_with("/WriteFile") {
                Ok(tonic::server::Grpc::new(
                    tonic_prost::ProstCodec::<FsWrite, WriteFileRequest>::default(),
                )
                .client_streaming(handler, request)
                .await)
            } else if path.ends_with("/ListFs") {
                Ok(tonic::server::Grpc::new(
                    tonic_prost::ProstCodec::<FsList, FsListRequest>::default(),
                )
                .unary(handler, request)
                .await)
            } else {
                assert!(path.ends_with("/ListCapabilities"));
                Ok(tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                    CapabilityList,
                    ListCapabilitiesRequest,
                >::default())
                .unary(handler, request)
                .await)
            }
        })
    }
}

struct RunningServer(tokio::task::JoinHandle<Result<(), tonic::transport::Error>>);
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct FailingSource(bool);

#[tokio::test]
async fn ordinary_deadline_bounds_partial_unary_body_when_server_ignores_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = futures_util::stream::unfold(listener, |listener| async {
        Some((listener.accept().await.map(|(socket, _)| socket), listener))
    });
    let _server = RunningServer(tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(Pages::default())
            .serve_with_incoming(incoming),
    ));
    let endpoint = operon_core::runtime::NodeEndpoint {
        node_id: "local".into(),
        endpoint: format!("grpc://{address}"),
        token: Some("test-token".into()),
        transport: operon_core::runtime::TransportConfig {
            rpc_timeout_secs: 1,
            ..Default::default()
        },
    };
    let mut client = operon_grpc_client::connect(&endpoint).await.unwrap();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.health(operon_grpc_client::request(&endpoint, HealthRequest {}).unwrap()),
    )
    .await
    .expect("client deadline must cover the body independently of the server")
    .unwrap_err();
    assert_eq!(error.code(), tonic::Code::DeadlineExceeded);
    let context = operon_core::RequestContext {
        run_id: Some("run-pages".into()),
        step_id: Some("step-pages".into()),
    };
    let request = operon_grpc_client::with_deadline(
        operon_grpc_client::request_with_context(
            &endpoint,
            Some(&context),
            FsListRequest {
                path: "/pages".into(),
                page_size: 1000,
                page_token: String::new(),
            },
        )
        .unwrap(),
        Some(std::time::Duration::from_secs(1)),
    );
    client.list_fs(request).await.unwrap();
}

async fn forward_or_blackhole(
    mut source: impl tokio::io::AsyncRead + Unpin,
    mut destination: impl tokio::io::AsyncWrite + Unpin,
    blackhole: Arc<AtomicBool>,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buffer = [0_u8; 16384];
    loop {
        let count = source.read(&mut buffer).await?;
        if count == 0 {
            return destination.shutdown().await;
        }
        if !blackhole.load(Ordering::SeqCst) {
            destination.write_all(&buffer[..count]).await?;
        }
    }
}

#[tokio::test]
async fn keepalive_detects_blackholed_transport_without_rpc_deadline_and_recovers() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = futures_util::stream::unfold(listener, |listener| async {
        Some((listener.accept().await.map(|(socket, _)| socket), listener))
    });
    let _server = RunningServer(tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(Pages::default())
            .serve_with_incoming(incoming),
    ));
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_address = proxy_listener.local_addr().unwrap();
    let blackhole = Arc::new(AtomicBool::new(false));
    let drop_packets = blackhole.clone();
    let _proxy = RunningServer(tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                connection = proxy_listener.accept() => {
                    let (client, _) = connection.unwrap();
                    let drop_packets = drop_packets.clone();
                    connections.spawn(async move {
                        let remote = tokio::net::TcpStream::connect(address).await?;
                        let (client_read, client_write) = client.into_split();
                        let (remote_read, remote_write) = remote.into_split();
                        tokio::try_join!(
                            forward_or_blackhole(client_read, remote_write, drop_packets.clone()),
                            forward_or_blackhole(remote_read, client_write, drop_packets),
                        )?;
                        Ok::<_, std::io::Error>(())
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    }));
    let endpoint = operon_core::runtime::NodeEndpoint {
        node_id: "local".into(),
        endpoint: format!("grpc://{proxy_address}"),
        token: Some("test-token".into()),
        transport: operon_core::runtime::TransportConfig {
            rpc_timeout_secs: 0,
            keepalive_interval_secs: 1,
            keepalive_timeout_secs: 1,
            ..Default::default()
        },
    };
    let context = operon_core::RequestContext {
        run_id: Some("run-pages".into()),
        step_id: Some("step-pages".into()),
    };
    let request = || {
        operon_grpc_client::request_with_context(
            &endpoint,
            Some(&context),
            FsListRequest {
                path: "/pages".into(),
                page_size: 1000,
                page_token: String::new(),
            },
        )
        .unwrap()
    };
    let mut client = operon_grpc_client::connect(&endpoint).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), client.list_fs(request()))
        .await
        .unwrap()
        .unwrap();
    blackhole.store(true, Ordering::SeqCst);
    let error = tokio::time::timeout(std::time::Duration::from_secs(6), client.list_fs(request()))
        .await
        .expect("keepalive must bound transport blackholes even when RPC deadlines are disabled")
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    blackhole.store(false, Ordering::SeqCst);
    // Reconnect explicitly: this is not an automatic mutation replay.
    let mut recovered = operon_grpc_client::connect(&endpoint).await.unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        recovered.list_fs(request()),
    )
    .await
    .unwrap()
    .unwrap();
}

impl tokio::io::AsyncRead for FailingSource {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.0 {
            Poll::Ready(Err(std::io::Error::other("injected upload source failure")))
        } else {
            self.0 = true;
            buffer.put_slice(b"PREFIX");
            Poll::Ready(Ok(()))
        }
    }
}

#[tokio::test]
async fn cli_transfer_liveness_bounds_stalls_preserves_slow_progress_and_never_replays() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = futures_util::stream::unfold(listener, |listener| async {
        Some((listener.accept().await.map(|(socket, _)| socket), listener))
    });
    let pages = Pages::default();
    let _server = RunningServer(tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(pages.clone())
            .serve_with_incoming(incoming),
    ));
    let mut endpoint = operon_core::runtime::NodeEndpoint {
        node_id: "local".into(),
        endpoint: format!("grpc://{address}"),
        token: Some("test-token".into()),
        transport: Default::default(),
    };
    endpoint.transport.progress_timeout_secs = 1;
    endpoint.transport.transfer_timeout_secs = 0;
    crate::grpc::with_request_context(
        operon_core::RequestContext {
            run_id: Some("run-pages".into()),
            step_id: Some("step-pages".into()),
        },
        || async {
            for path in ["/stall-headers", "/stall-body"] {
                let mut output = Vec::new();
                let error = crate::grpc_fs::read_file_to_writer(&endpoint, path, &mut output)
                    .await
                    .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<Status>().unwrap().code(),
                    tonic::Code::DeadlineExceeded
                );
                assert_eq!(
                    output,
                    if path == "/stall-body" {
                        vec![1]
                    } else {
                        vec![]
                    }
                );
            }
            let mut output = Vec::new();
            crate::grpc_fs::read_file_to_writer(&endpoint, "/slow", &mut output).await?;
            assert_eq!(output, (0..12).collect::<Vec<u8>>());
            let before = pages.calls.load(Ordering::SeqCst);
            let error = crate::grpc_fs::write_file_bytes(&endpoint, "/never-ack", &[1, 2, 3], None)
                .await
                .unwrap_err();
            assert_eq!(
                error.downcast_ref::<Status>().unwrap().code(),
                tonic::Code::DeadlineExceeded
            );
            assert_eq!(pages.calls.load(Ordering::SeqCst), before + 1);
            let error = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                crate::grpc_fs::write_reader(
                    &endpoint,
                    "/source-error",
                    FailingSource(false),
                    None,
                ),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert!(format!("{error:#}").contains("injected upload source failure"));
            // A cancelled/stalled request does not poison the next connection.
            let mut output = Vec::new();
            crate::grpc_fs::read_file_to_writer(&endpoint, "/slow", &mut output).await?;
            assert_eq!(output.len(), 12);
            Ok(())
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn cli_pagination_reuses_connections_preserves_context_and_page_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let connections = Arc::new(AtomicUsize::new(0));
    let accepted = connections.clone();
    let incoming =
        futures_util::stream::unfold((listener, accepted), |(listener, accepted)| async {
            let socket = listener.accept().await.map(|(socket, _)| {
                accepted.fetch_add(1, Ordering::SeqCst);
                socket
            });
            Some((socket, (listener, accepted)))
        });
    let pages = Pages::default();
    let _server = RunningServer(tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(pages.clone())
            .serve_with_incoming(incoming),
    ));
    let endpoint = operon_core::runtime::NodeEndpoint {
        transport: Default::default(),
        node_id: "local".into(),
        endpoint: format!("grpc://{address}"),
        token: Some("test-token".into()),
    };
    crate::grpc::with_request_context(
        operon_core::RequestContext {
            run_id: Some("run-pages".into()),
            step_id: Some("step-pages".into()),
        },
        || async {
            let files = crate::grpc_fs::fs_list(&endpoint, "/pages").await?;
            assert_eq!(
                files
                    .entries
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>(),
                ["file-0", "file-1", "file-2"]
            );
            assert!(files.next_page_token.is_empty());
            assert_eq!(connections.load(Ordering::SeqCst), 1);
            assert_eq!(
                crate::grpc::list_capabilities(&endpoint)
                    .await?
                    .capabilities
                    .len(),
                3
            );
            assert_eq!(connections.load(Ordering::SeqCst), 2);
            pages.fail.store(true, Ordering::SeqCst);
            let error = crate::grpc_fs::fs_list(&endpoint, "/pages")
                .await
                .unwrap_err();
            assert!(error.to_string().contains("page failure"));
            assert_eq!(connections.load(Ordering::SeqCst), 3);
            assert_eq!(pages.calls.load(Ordering::SeqCst), 9);
            pages.fail.store(false, Ordering::SeqCst);
            pages.delay_ms.store(100, Ordering::SeqCst);
            let mut client = operon_grpc_client::connect(&endpoint).await?;
            let message = || FsListRequest {
                path: "/pages".into(),
                page_size: 1000,
                page_token: String::new(),
            };
            let request = operon_grpc_client::with_deadline(
                crate::grpc::with_auth(&endpoint, message())?,
                Some(std::time::Duration::from_millis(20)),
            );
            let error = client.list_fs(request).await.unwrap_err();
            assert!(
                matches!(
                    error.code(),
                    tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
                ),
                "{error}"
            );
            let request = operon_grpc_client::with_deadline(
                crate::grpc::with_auth(&endpoint, message())?,
                Some(std::time::Duration::from_secs(1)),
            );
            assert_eq!(client.list_fs(request).await?.into_inner().entries.len(), 1);
            Ok(())
        },
    )
    .await
    .unwrap();
}
