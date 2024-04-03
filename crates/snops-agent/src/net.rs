use std::net::IpAddr;

use anyhow::Result;
use tracing::info;

pub fn get_internal_addrs() -> Result<Vec<IpAddr>> {
    let network_interfaces = local_ip_address::list_afinet_netifas()?;

    Ok(network_interfaces
        .into_iter()
        .filter_map(|(name, ip)| {
            // loopback addresses can be used when the networks are calculated
            // to be the same, but they are not useful for peer to peer comms
            if ip.is_loopback() {
                info!("skipping loopback iface {name}: {ip:?}");
                return None;
            }

            // ignore link-local addresses
            // https://en.wikipedia.org/wiki/Link-local_address
            // these addrs are about as useful as their v4 counterpart
            if let IpAddr::V6(v6) = ip {
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    info!("skipping link-local iface {name}: {ip:?}");
                    return None;
                }
            }
            info!("using iface {name}: {ip:?}");
            Some(ip)
        })
        .collect())
}

pub async fn get_external_addr() -> Option<IpAddr> {
    // default behavior of the external_ip::get_ip function uses
    // dns sources, which have given me addresses that resolve to
    // networks that are not mine...

    let sources: external_ip::Sources = external_ip::get_http_sources();
    let consensus = external_ip::ConsensusBuilder::new()
        .add_sources(sources)
        .build();
    consensus.get_consensus().await
}
// 172.253.198.135
// 172.217.38.175
