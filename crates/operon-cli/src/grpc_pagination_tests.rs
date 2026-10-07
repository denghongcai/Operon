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

#[derive(Clone, Default)]
struct Pages {
    calls: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
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
            if path.ends_with("/ListFs") {
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
            Ok(())
        },
    )
    .await
    .unwrap();
}
