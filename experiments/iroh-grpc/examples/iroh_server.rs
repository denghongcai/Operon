//! Container-mode server: serves tonic Probe over an iroh stream on all
//! interfaces, printing its endpoint id and observed direct addresses first.
use operon_iroh_grpc_poc::{
    QuicIo, demo::Echo, network_endpoint, probe::probe_server::ProbeServer,
};
use std::{sync::Arc, time::Duration};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = network_endpoint().await?;
    // Direct addresses appear once the endpoint observes its local interfaces.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let addr = endpoint.addr();
        let ips: Vec<_> = addr.ip_addrs().copied().collect();
        if !ips.is_empty() {
            for ip in &ips {
                println!("IROH_IP_ADDR={ip}");
            }
            break;
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "no direct address observed before deadline"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("IROH_NODE_ID={}", endpoint.id());
    use std::io::Write;
    std::io::stdout().flush()?;

    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let accept_endpoint = endpoint.clone();
    let accept = tokio::spawn(async move {
        let slots = Arc::new(tokio::sync::Semaphore::new(8));
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                incoming = accept_endpoint.accept() => {
                    let Some(incoming) = incoming else { break; };
                    let Ok(slot) = slots.clone().try_acquire_owned() else {
                        incoming.refuse();
                        continue;
                    };
                    let tx = tx.clone();
                    tasks.spawn(async move {
                        let _slot = slot;
                        let conn = incoming.await?;
                        let (send, recv) = conn.accept_bi().await?;
                        tx.send(Ok::<_, std::io::Error>(QuicIo::new(conn, send, recv))).await?;
                        Ok::<_, anyhow::Error>(())
                    });
                }
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    match result {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => eprintln!("rejected connection: {error}"),
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
        tasks.abort_all();
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                return Err(error.into());
            }
        }
        Ok::<_, anyhow::Error>(())
    });
    let shutdown_endpoint = endpoint.clone();
    let result = Server::builder()
        .add_service(
            ProbeServer::new(Echo)
                .max_decoding_message_size(16 * 1024 * 1024)
                .max_encoding_message_size(16 * 1024 * 1024),
        )
        .serve_with_incoming_shutdown(ReceiverStream::new(rx), async move {
            tokio::signal::ctrl_c()
                .await
                .expect("install SIGINT handler");
            shutdown_endpoint.close().await;
        })
        .await;
    endpoint.close().await;
    accept.await??;
    result?;
    println!("PASS server accept tasks joined");
    Ok(())
}
