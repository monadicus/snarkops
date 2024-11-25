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
                info!("Skipping loopback iface {name}: {ip:?}");
                return None;
            }

            // ignore link-local addresses
            // https://en.wikipedia.org/wiki/Link-local_address
            // these addrs are about as useful as their v4 counterpart
            if let IpAddr::V6(v6) = ip {
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    info!("Skipping link-local iface {name}: {ip:?}");
                    return None;
                }
            }
            info!("Using iface {name}: {ip:?}");
            Some(ip)
        })
        .collect())
}
