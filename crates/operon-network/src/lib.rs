use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use operon_core::{DiscoveryList, DiscoveryRecord, ServiceCheck, ServiceDefinition};

pub use operon_config::{NetworkProviderKind, NodeEndpoint};

pub const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

pub fn discover_lan_nodes(timeout: Duration) -> anyhow::Result<DiscoveryList> {
    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse(OPERON_MDNS_SERVICE)?;
    let deadline = Instant::now() + timeout;
    let mut records = BTreeMap::new();
    let mut fullnames = BTreeMap::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let record = discovery_record_from_info(&info);
                fullnames.insert(info.get_fullname().to_string(), record.node_id.clone());
                records.insert(record.node_id.clone(), record);
            }
            Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                remove_discovered_service(&mut records, &mut fullnames, &fullname);
            }
            Ok(_) => {}
            Err(error) => {
                if Instant::now() >= deadline {
                    break;
                }
                anyhow::bail!("mDNS discovery receiver failed: {error}");
            }
        }
    }
    Ok(DiscoveryList {
        nodes: records.into_values().collect(),
    })
}

pub async fn check_tcp_service(service: &ServiceDefinition, timeout: Duration) -> ServiceCheck {
    let started = Instant::now();
    let result = tokio::time::timeout(
        timeout,
        tokio::net::TcpStream::connect((service.host.as_str(), service.port)),
    )
    .await;
    let latency_ms = started.elapsed().as_millis();
    let (ok, reason) = match result {
        Ok(Ok(_)) => (true, None),
        Ok(Err(error)) => (false, Some(error.to_string())),
        Err(_) => (false, Some("service check timed out".to_string())),
    };

    ServiceCheck {
        id: service.id.clone(),
        ok,
        latency_ms,
        reason,
    }
}

pub async fn check_udp_service(service: &ServiceDefinition, timeout: Duration) -> ServiceCheck {
    let started = Instant::now();
    let address = format!("{}:{}", service.host, service.port);
    let result = tokio::time::timeout(timeout, connect_udp_socket(&address)).await;
    let latency_ms = started.elapsed().as_millis();
    let (ok, reason) = match result {
        Ok(Ok(_)) => (true, None),
        Ok(Err(error)) => (false, Some(error.to_string())),
        Err(_) => (false, Some("service check timed out".to_string())),
    };

    ServiceCheck {
        id: service.id.clone(),
        ok,
        latency_ms,
        reason,
    }
}

async fn connect_udp_socket(address: &str) -> std::io::Result<tokio::net::UdpSocket> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    match socket.connect(address).await {
        Ok(()) => Ok(socket),
        Err(ipv4_error) => {
            let socket = tokio::net::UdpSocket::bind("[::]:0").await?;
            socket.connect(address).await.map_err(|_| ipv4_error)?;
            Ok(socket)
        }
    }
}

fn remove_discovered_service(
    records: &mut BTreeMap<String, DiscoveryRecord>,
    fullnames: &mut BTreeMap<String, String>,
    fullname: &str,
) {
    if let Some(node_id) = fullnames.remove(fullname) {
        records.remove(&node_id);
    }
}

fn discovery_record_from_info(info: &ResolvedService) -> DiscoveryRecord {
    let node_id = info
        .get_property_val_str("node_id")
        .unwrap_or(info.get_fullname())
        .trim_end_matches(OPERON_MDNS_SERVICE)
        .trim_end_matches('.')
        .to_string();
    let fallback_endpoint = info
        .get_addresses_v4()
        .into_iter()
        .next()
        .map(|addr| format!("grpc://{}:{}", addr, info.get_port()))
        .unwrap_or_else(|| format!("grpc://{}:{}", info.get_hostname(), info.get_port()));
    let endpoint = info
        .get_property_val_str("endpoint")
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback_endpoint);
    let capabilities = info
        .get_property_val_str("capabilities")
        .unwrap_or("")
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();

    DiscoveryRecord {
        node_id,
        endpoint,
        provider: "lan".to_string(),
        capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_removed_event_removes_discovered_record() {
        let mut records = BTreeMap::from([(
            "node-a".to_string(),
            DiscoveryRecord {
                node_id: "node-a".to_string(),
                endpoint: "grpc://127.0.0.1:7788".to_string(),
                provider: "lan".to_string(),
                capabilities: Vec::new(),
            },
        )]);
        let mut fullnames = BTreeMap::from([(
            "node-a._operon._tcp.local.".to_string(),
            "node-a".to_string(),
        )]);

        remove_discovered_service(&mut records, &mut fullnames, "node-a._operon._tcp.local.");

        assert!(records.is_empty());
        assert!(fullnames.is_empty());
    }
}
