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
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let record = discovery_record_from_info(&info);
                records.insert(record.node_id.clone(), record);
            }
            Ok(_) => {}
            Err(_) => {}
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
