use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use operon_core::{DiscoveryList, DiscoveryRecord, ServiceCheck, ServiceDefinition};

pub use operon_config::NodeEndpoint;

pub const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";
const UDP_SOCKET_CONNECT_REASON: &str = "udp socket connected; datagram response not verified";

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
        Ok(Ok(_)) => (true, Some("tcp service reachable".to_string())),
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
        Ok(Ok(_)) => (true, Some(UDP_SOCKET_CONNECT_REASON.to_string())),
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
        capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operon_core::{ServicePermissions, ServiceProtocol};
    use tokio::net::TcpListener;

    #[test]
    fn service_removed_event_removes_discovered_record() {
        let mut records = BTreeMap::from([(
            "node-a".to_string(),
            DiscoveryRecord {
                node_id: "node-a".to_string(),
                endpoint: "grpc://127.0.0.1:7788".to_string(),
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

    #[test]
    fn resolved_mdns_record_yields_endpoint_candidate_without_provider_metadata() {
        let info = mdns_sd::ServiceInfo::new(
            OPERON_MDNS_SERVICE,
            "gpu",
            "gpu.local.",
            "10.0.0.8",
            7789,
            &[
                ("node_id", "gpu"),
                ("endpoint", "grpc://10.0.0.8:7789"),
                ("capabilities", "fs:workspace,exec:default"),
                ("provider", "tailscale"),
            ][..],
        )
        .expect("service info")
        .as_resolved_service();

        let record = discovery_record_from_info(&info);

        assert_eq!(record.node_id, "gpu");
        assert_eq!(record.endpoint, "grpc://10.0.0.8:7789");
        assert_eq!(
            record.capabilities,
            vec!["fs:workspace".to_string(), "exec:default".to_string()]
        );
    }

    #[tokio::test]
    async fn tcp_service_check_reports_tcp_reachability_on_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        let accept = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let check = check_tcp_service(
            &test_service("tcp-ok", port, ServiceProtocol::Tcp),
            Duration::from_secs(1),
        )
        .await;

        assert!(check.ok);
        assert_eq!(check.reason.as_deref(), Some("tcp service reachable"));
        accept.await.expect("accept task");
    }

    #[tokio::test]
    async fn tcp_service_check_reports_connect_failures() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let check = check_tcp_service(
            &test_service("tcp-refused", port, ServiceProtocol::Tcp),
            Duration::from_secs(1),
        )
        .await;

        assert!(!check.ok);
        assert!(check
            .reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty()));
    }

    #[tokio::test]
    async fn udp_service_check_reports_socket_connect_success() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("udp socket");
        let port = socket.local_addr().expect("local addr").port();

        let check = check_udp_service(
            &test_service("udp-ok", port, ServiceProtocol::Udp),
            Duration::from_secs(1),
        )
        .await;

        assert!(check.ok);
        assert_eq!(check.reason.as_deref(), Some(UDP_SOCKET_CONNECT_REASON));
    }

    #[tokio::test]
    async fn udp_service_check_reports_resolution_failures() {
        let mut service = test_service("udp-bad", 53, ServiceProtocol::Udp);
        service.host = "not a valid host name".to_string();

        let check = check_udp_service(&service, Duration::from_secs(1)).await;

        assert!(!check.ok);
        assert!(check
            .reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty()));
    }

    fn test_service(id: &str, port: u16, protocol: ServiceProtocol) -> ServiceDefinition {
        ServiceDefinition {
            id: id.to_string(),
            name: id.to_string(),
            host: "127.0.0.1".to_string(),
            port,
            protocol,
            description: String::new(),
            permissions: ServicePermissions {
                check: true,
                forward: true,
            },
        }
    }
}
