use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

/// Get an available port on the local machine.
pub fn get_available_port() -> Option<u16> {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}
