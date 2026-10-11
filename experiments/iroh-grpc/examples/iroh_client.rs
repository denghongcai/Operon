//! Container-mode client: dials a server endpoint by id plus explicit direct
//! IP addresses (no relay, no discovery) and verifies gRPC behavior.
use iroh::{EndpointAddr, TransportAddr};
use operon_iroh_grpc_poc::{
    channel, network_endpoint,
    probe::{Frame, probe_client::ProbeClient},
};
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let node_id: iroh::EndpointId = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: iroh_client <NODE_ID> <IP_ADDR>..."))?
        .parse()?;
    let ip_addrs = args
        .map(|raw| Ok::<_, anyhow::Error>(TransportAddr::Ip(raw.parse()?)))
        .collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(!ip_addrs.is_empty(), "at least one direct address required");
    let target = EndpointAddr::from_parts(node_id, ip_addrs);

    let endpoint = network_endpoint().await?;
    let grpc = channel(endpoint.clone(), target, Duration::from_secs(10)).await?;
    let mut client = ProbeClient::new(grpc)
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
    println!("PASS cross-container unary >8MiB exact payload");

    let mut calls = tokio::task::JoinSet::new();
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
    println!("PASS cross-container 64 concurrent verified calls");

    let (tx, rx) = tokio::sync::mpsc::channel(2);
    let mut stream = client.duplex(ReceiverStream::new(rx)).await?.into_inner();
    for i in 0..32u8 {
        let data = vec![i; 65536];
        tx.send(Frame { data: data.clone() }).await?;
        assert_eq!(stream.message().await?.expect("response").data, data);
    }
    drop(tx);
    assert!(stream.message().await?.is_none());
    println!("PASS cross-container interactive bidirectional streaming");

    drop(client);
    endpoint.close().await;
    Ok(())
}
