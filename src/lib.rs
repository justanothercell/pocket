#![feature(try_blocks)]

use std::{error::Error, net::{IpAddr, SocketAddr, SocketAddrV4}};

use dns_lookup::lookup_host;

pub mod client;
pub mod server;
pub mod protocol;

pub type AnyError = Box<dyn Error>;

pub fn parse_address(address: &str, default_port: Option<u16>) -> Result<SocketAddr, AnyError> {
    let (address, port) = address
        .split_once(':')
        .map(|(address, port)| Ok::<_, AnyError>((address, Some(port.parse()?))))
        .transpose()?
        .unwrap_or((address, default_port));
    let address = lookup_host(address)?.
        into_iter().find_map(|addr| match addr {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        }).ok_or("Unable to resolve host")?;
    Ok(SocketAddr::V4(SocketAddrV4::new(address, port.ok_or("no port specified")?)))
}
