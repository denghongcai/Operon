use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use mdns_sd::{ServiceDaemon, ServiceInfo};
use operon_core::CapabilityList;

const OPERON_MDNS_SERVICE: &str = "_operon._tcp.local.";

pub(crate) fn advertise_lan(
    node_id: &str,
    listen: SocketAddr,
    capabilities: &CapabilityList,
) -> anyhow::Result<ServiceDaemon> {
    let mdns = ServiceDaemon::new()?;
    let capability_summary = capabilities
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let endpoint = if listen.ip().is_unspecified() {
        String::new()
    } else {
        format!("grpc://{}:{}", advertised_host(listen.ip()), listen.port())
    };
    let properties = [
        ("node_id", node_id),
        ("provider", "lan"),
        ("endpoint", endpoint.as_str()),
        ("capabilities", capability_summary.as_str()),
    ];
    let service = ServiceInfo::new(
        OPERON_MDNS_SERVICE,
        node_id,
        &format!("{}.local.", node_id),
        "",
        listen.port(),
        &properties[..],
    )?
    .enable_addr_auto();
    mdns.register(service)?;
    Ok(mdns)
}

fn advertised_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) if ip == Ipv4Addr::UNSPECIFIED => "127.0.0.1".to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        ip => ip.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_addresses_advertise_localhost() {
        assert_eq!(
            advertised_host(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            "127.0.0.1"
        );
    }
}
