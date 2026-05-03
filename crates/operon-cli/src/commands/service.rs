use std::{net::SocketAddr, path::PathBuf};

use operon_core::{ServiceCheck, ServiceList, ServiceProtocol};

use crate::{
    grpc, grpc_service,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

pub(crate) async fn list(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let list: ServiceList = grpc::list_services(&endpoint).await?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for service in list.services {
        println!(
            "{}\t{}\t{}:{}\t{}",
            service.id,
            service.name,
            service.host,
            service.port,
            format_service_protocol(&service.protocol)
        );
    }
    Ok(())
}

pub(crate) async fn check(
    config_path: PathBuf,
    node_id: &str,
    service_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let check: ServiceCheck = grpc::check_service(&endpoint, service_id).await?;
    if output.json {
        print_json(&check)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    println!(
        "{} ok={} latency_ms={} reason={}",
        check.id,
        check.ok,
        check.latency_ms,
        check.reason.as_deref().unwrap_or("-")
    );
    Ok(())
}

pub(crate) async fn forward(
    config_path: PathBuf,
    node_id: String,
    service_id: String,
    listen: SocketAddr,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, &node_id)?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let local_addr = listener.local_addr()?;
    if output.json {
        print_json(&serde_json::json!({
            "node_id": node_id,
            "service_id": service_id,
            "listen": local_addr.to_string(),
        }))?;
    } else if !output.quiet {
        println!("forwarding {} -> {}:{}", local_addr, node_id, service_id);
    }

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        let endpoint = endpoint.clone();
        let service_id = service_id.clone();
        tokio::spawn(async move {
            if let Err(error) =
                grpc_service::forward_service_connection(&endpoint, &service_id, socket).await
            {
                eprintln!(
                    "service forward connection from {} failed: {:#}",
                    peer_addr, error
                );
            }
        });
    }
}

pub(crate) async fn forward_udp(
    config_path: PathBuf,
    node_id: String,
    service_id: String,
    listen: SocketAddr,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, &node_id)?;
    let socket = tokio::net::UdpSocket::bind(listen).await?;
    let local_addr = socket.local_addr()?;
    if output.json {
        print_json(&serde_json::json!({
            "node_id": node_id,
            "service_id": service_id,
            "listen": local_addr.to_string(),
            "protocol": "udp",
        }))?;
    } else if !output.quiet {
        println!(
            "forwarding udp {} -> {}:{}",
            local_addr, node_id, service_id
        );
    }

    grpc_service::forward_service_datagrams(&endpoint, &service_id, socket).await
}

pub(crate) fn format_service_protocol(protocol: &ServiceProtocol) -> &'static str {
    match protocol {
        ServiceProtocol::Tcp => "tcp",
        ServiceProtocol::Udp => "udp",
    }
}
