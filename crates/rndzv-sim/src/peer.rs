use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::nat::NatModel;
use crate::runner::SimRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimPeerId(pub u32);

pub struct SimPeer {
    pub id: SimPeerId,
    pub name: String,
    pub local_addr: SocketAddr,
    pub nat: Option<Box<dyn NatModel>>,
    pub runner: SimRunner,
    pub inbox: Vec<(u64, SocketAddr, Vec<u8>)>,
    pub public_addr: SocketAddr,
    pub public_port_base: u16,
}

impl SimPeer {
    pub fn new(name: &str, nat: Option<Box<dyn NatModel>>, token: rift_rndzv::Srt) -> Self {
        let id = SimPeerId(fnv_hash(name.as_bytes()));
        let local_ip = Ipv4Addr::new(10, 0, (id.0 % 250) as u8, 1);
        let public_port_base = 40000u16.wrapping_add((id.0 % 1000) as u16);
        let public_ip = if nat.is_none() {
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, (id.0 % 200) as u8))
        } else {
            IpAddr::V4(Ipv4Addr::new(10, 0, (id.0 % 250) as u8, 1))
        };
        let public_addr = SocketAddr::new(public_ip, public_port_base);

        Self {
            id,
            name: name.to_string(),
            local_addr: SocketAddr::new(IpAddr::V4(local_ip), 40000 + (id.0 as u16 % 1000)),
            public_addr,
            public_port_base,
            nat,
            runner: SimRunner::new(token),
            inbox: Vec::new(),
        }
    }
}

fn fnv_hash(bytes: &[u8]) -> u32 {
    let mut hash = 2166136261u32;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}
