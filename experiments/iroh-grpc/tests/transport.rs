use anyhow::Result;
use operon_iroh_grpc_poc::{
    QuicIo, channel, local_endpoint,
    probe::{
        Frame,
        probe_client::ProbeClient,
        probe_server::{Probe, ProbeServer},
    },
};
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming, transport::Server};

#[derive(Clone, Default)]
struct Service {
    hanging: Arc<AtomicUsize>,
}
struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[tonic::async_trait]
impl Probe for Service {
    async fn echo(&self, request: Request<Frame>) -> Result<Response<Frame>, Status> {
        // Transport-authenticated peer identity reaches the handler.
        if request.extensions().get::<iroh::EndpointId>().is_none() {
            return Err(Status::unauthenticated("missing peer identity"));
        }
        Ok(Response::new(request.into_inner()))
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
    async fn hang(&self, _: Request<Frame>) -> Result<Response<Frame>, Status> {
        self.hanging.fetch_add(1, Ordering::SeqCst);
        let _guard = Active(self.hanging.clone());
        std::future::pending().await
    }
}

async fn bounded<F: std::future::Future<Output = Result<()>>>(future: F) -> Result<()> {
    let secs = std::env::var("OPERON_POC_TEST_TIMEOUT_SECS")
        .unwrap_or_else(|_| "60".into())
        .parse::<u64>()?;
    anyhow::ensure!(secs > 0, "test timeout must be positive");
    tokio::time::timeout(Duration::from_secs(secs), future).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_tonic_over_iroh() -> Result<()> {
    bounded(async {
        let server = local_endpoint().await?;
        let client_endpoint = local_endpoint().await?;
        let accepted_peer = client_endpoint.id();
        let (incoming_tx, incoming_rx) = mpsc::channel(8);
        let accept_endpoint = server.clone();
        let accept = tokio::spawn(async move {
            while let Some(incoming) = accept_endpoint.accept().await {
                let conn = incoming.await?;
                anyhow::ensure!(conn.remote_id() == accepted_peer, "unexpected peer");
                let (send, recv) = conn.accept_bi().await?;
                incoming_tx
                    .send(Ok::<_, std::io::Error>(QuicIo::new(conn, send, recv)))
                    .await?;
            }
            Ok::<_, anyhow::Error>(())
        });
        let service = Service::default();
        let hanging = service.hanging.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let serve = tokio::spawn(
            Server::builder()
                .add_service(
                    ProbeServer::new(service)
                        .max_decoding_message_size(16 * 1024 * 1024)
                        .max_encoding_message_size(16 * 1024 * 1024),
                )
                .serve_with_incoming_shutdown(ReceiverStream::new(incoming_rx), async {
                    let _ = shutdown_rx.await;
                }),
        );
        let channel = channel(
            client_endpoint.clone(),
            server.addr(),
            Duration::from_secs(10),
        )
        .await?;
        let mut client = ProbeClient::new(channel.clone())
            .max_decoding_message_size(16 * 1024 * 1024)
            .max_encoding_message_size(16 * 1024 * 1024);
        let payload: Vec<u8> = (0..8 * 1024 * 1024 + 17).map(|i| (i % 251) as u8).collect();
        assert_eq!(
            client
                .echo(Frame {
                    data: payload.clone()
                })
                .await?
                .into_inner()
                .data,
            payload
        );
        println!("PASS unary >8MiB exact payload, authenticated peer extension");
        let mut calls = tokio::task::JoinSet::new();
        let started = Instant::now();
        for i in 0..64u8 {
            let mut c = client.clone();
            calls.spawn(async move {
                assert_eq!(
                    c.echo(Frame {
                        data: vec![i; 4096]
                    })
                    .await?
                    .into_inner()
                    .data,
                    vec![i; 4096]
                );
                Ok::<_, anyhow::Error>(())
            });
        }
        while let Some(call) = calls.join_next().await {
            call??;
        }
        println!(
            "PASS 64 concurrent verified 4KiB calls; reference ops/s={:.3}",
            64.0 / started.elapsed().as_secs_f64()
        );
        let (tx, rx) = mpsc::channel(2);
        let mut stream = client.duplex(ReceiverStream::new(rx)).await?.into_inner();
        // Interactive ping-pong: server cannot wait for client EOF to reply.
        for i in 0..32u8 {
            let data = vec![i; 65536];
            tx.send(Frame { data: data.clone() }).await?;
            assert_eq!(stream.message().await?.expect("response").data, data);
        }
        drop(tx);
        assert!(stream.message().await?.is_none());
        println!("PASS interactive bidirectional streaming and half-close");
        let mut req = Request::new(Frame::default());
        req.set_timeout(Duration::from_millis(150));
        let err = client.hang(req).await.unwrap_err();
        assert!(matches!(
            err.code(),
            tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
        ));
        while hanging.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
        println!("PASS RPC deadline cancels server handler");
        let mut c = client.clone();
        let call = tokio::spawn(async move { c.hang(Frame::default()).await });
        while hanging.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        while hanging.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
        println!("PASS caller abort with no RPC deadline cancels server handler");
        let mut c = client.clone();
        let call = tokio::spawn(async move { c.hang(Frame::default()).await });
        while hanging.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
        client_endpoint.close().await;
        assert!(call.await?.is_err());
        while hanging.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
        println!("PASS endpoint shutdown terminates deadline-zero RPC");
        drop(client);
        drop(channel);
        shutdown_tx.send(()).expect("server alive");
        serve.await??;
        server.close().await;
        accept.await??;
        println!("PASS owned accept/server tasks joined");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn wrong_alpn_is_rejected() -> Result<()> {
    bounded(async {
        let server = local_endpoint().await?;
        let client = local_endpoint().await?;
        let endpoint = server.clone();
        let accept = tokio::spawn(async move {
            if let Some(incoming) = endpoint.accept().await {
                let _ = incoming.await;
            }
        });
        assert!(
            tokio::time::timeout(
                Duration::from_secs(5),
                client.connect(server.addr(), b"not-operon")
            )
            .await?
            .is_err()
        );
        client.close().await;
        server.close().await;
        accept.await?;
        println!("PASS wrong ALPN rejected");
        Ok(())
    })
    .await
}
