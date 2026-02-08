use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use get_if_addrs::get_if_addrs;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};

use rift_core::PeerId;
use rift_metrics as metrics;
use tracing::debug;
use rand::RngCore;

#[derive(Debug, Clone)]
pub struct NatConfig {
    pub local_ports: Vec<u16>,
    pub stun_servers: Vec<SocketAddr>,
    pub stun_timeout_ms: u64,
    pub punch_interval_ms: u64,
    pub punch_timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    Unknown,
    OpenInternet,
    Natted,
}

#[derive(Debug, Clone)]
pub struct PeerEndpoint {
    pub peer_id: PeerId,
    pub external_addrs: Vec<SocketAddr>,
    pub punch_ports: Vec<u16>,
}

#[derive(Debug, thiserror::Error)]
pub enum HolePunchError {
    #[error("no local ports could be bound")]
    NoLocalPorts,
    #[error("no remote addresses to punch")]
    NoRemoteAddrs,
    #[error("timeout while punching")]
    Timeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum StunError {
    #[error("no stun servers configured")]
    NoServers,
    #[error("no stun responses received")]
    NoResponses,
    #[error("invalid stun response")]
    InvalidResponse,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

const PUNCH_SYN: &[u8] = b"RIFT_PUNCH";
const PUNCH_ACK: &[u8] = b"RIFT_ACK";
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;
const STUN_ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const KEEPALIVE_BYTES: &[u8] = b"RIFT_KEEPALIVE";

pub async fn attempt_hole_punch(
    nat_cfg: &NatConfig,
    peer: &PeerEndpoint,
) -> Result<(UdpSocket, SocketAddr), HolePunchError> {
    metrics::inc_counter("rift_hole_punch_attempts", &[]);
    let ports = if nat_cfg.local_ports.is_empty() {
        vec![0]
    } else {
        nat_cfg.local_ports.clone()
    };

    let mut sockets = Vec::new();
    for port in ports {
        if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await {
            sockets.push(socket);
        }
    }

    if sockets.is_empty() {
        debug!("hole punch failed: no local ports");
        metrics::inc_counter("rift_hole_punch_failures", &[("reason", "no_local_ports")]);
        return Err(HolePunchError::NoLocalPorts);
    }

    let target_addrs = build_target_addrs(peer);
    if target_addrs.is_empty() {
        debug!("hole punch failed: no remote addrs");
        metrics::inc_counter("rift_hole_punch_failures", &[("reason", "no_remote_addrs")]);
        return Err(HolePunchError::NoRemoteAddrs);
    }

    let punch_interval_ms = nat_cfg.punch_interval_ms;
    let done = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = mpsc::channel::<(UdpSocket, SocketAddr)>(1);

    for socket in sockets {
        let targets = target_addrs.clone();
        let done = done.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if done.load(Ordering::Relaxed) {
                return;
            }
            let mut tick = interval(Duration::from_millis(punch_interval_ms.max(50)));
            let mut buf = [0u8; 1024];

            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        if done.load(Ordering::Relaxed) {
                            return;
                        }
                        for addr in &targets {
                            let _ = socket.send_to(PUNCH_SYN, addr).await;
                        }
                    }
                    recv = socket.recv_from(&mut buf) => {
                        let Ok((len, addr)) = recv else { continue; };
                        if done.load(Ordering::Relaxed) {
                            return;
                        }
                        if !targets.contains(&addr) {
                            continue;
                        }
                        let data = &buf[..len];
                        if data == PUNCH_SYN {
                            let _ = socket.send_to(PUNCH_ACK, addr).await;
                        } else if data == PUNCH_ACK {
                            let _ = socket.send_to(PUNCH_ACK, addr).await;
                        }
                        done.store(true, Ordering::Relaxed);
                        let _ = tx.send((socket, addr)).await;
                        return;
                    }
                }
            }
        });
    }

    let timeout_ms = nat_cfg.punch_timeout_ms.max(500);
    let result = timeout(Duration::from_millis(timeout_ms), rx.recv()).await;
    match result {
        Ok(Some((socket, addr))) => {
            debug!(%addr, "hole punch success");
            metrics::inc_counter("rift_hole_punch_success", &[]);
            Ok((socket, addr))
        }
        _ => {
            debug!("hole punch timeout");
            metrics::inc_counter("rift_hole_punch_failures", &[("reason", "timeout")]);
            Err(HolePunchError::Timeout)
        }
    }
}

/// Collect local (host) candidates for a given listen port.
/// Loopback, unspecified, and link-local addresses are excluded.
pub fn gather_local_candidates(listen_port: u16) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            let ip = iface.ip();
            if ip.is_loopback() || ip.is_unspecified() {
                continue;
            }
            if let IpAddr::V6(v6) = ip {
                if v6.is_unicast_link_local() {
                    continue;
                }
            }
            addrs.push(SocketAddr::new(ip, listen_port));
        }
    }
    addrs.sort();
    addrs.dedup();
    addrs
}

pub fn detect_nat_type(local_addrs: &[SocketAddr], public_addrs: &[SocketAddr]) -> NatType {
    if public_addrs.is_empty() {
        return NatType::Unknown;
    }
    for public in public_addrs {
        if local_addrs.iter().any(|local| local == public) {
            return NatType::OpenInternet;
        }
    }
    NatType::Natted
}

pub async fn gather_public_addrs(nat_cfg: &NatConfig) -> Result<Vec<SocketAddr>, StunError> {
    if nat_cfg.stun_servers.is_empty() {
        return Err(StunError::NoServers);
    }
    let ports = if nat_cfg.local_ports.is_empty() {
        vec![0]
    } else {
        nat_cfg.local_ports.clone()
    };

    let mut results = Vec::new();
    for port in ports {
        for server in &nat_cfg.stun_servers {
            if let Ok(addr) = stun_binding_request(*server, port, nat_cfg.stun_timeout_ms).await {
                results.push(addr);
            }
        }
    }

    results.sort();
    results.dedup();
    if results.is_empty() {
        Err(StunError::NoResponses)
    } else {
        Ok(results)
    }
}

/// Spawn periodic keep-alive packets to keep NAT bindings warm.
pub fn spawn_keepalive(
    socket: Arc<UdpSocket>,
    targets: Vec<SocketAddr>,
    interval_ms: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if targets.is_empty() {
            return;
        }
        let mut tick = interval(Duration::from_millis(interval_ms.max(200)));
        loop {
            tick.tick().await;
            for addr in &targets {
                let _ = socket.send_to(KEEPALIVE_BYTES, addr).await;
            }
        }
    })
}

fn build_target_addrs(peer: &PeerEndpoint) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    for addr in &peer.external_addrs {
        addrs.push(*addr);
        for port in &peer.punch_ports {
            addrs.push(SocketAddr::new(addr.ip(), *port));
        }
    }
    addrs.sort();
    addrs.dedup();
    addrs
}

async fn stun_binding_request(
    server: SocketAddr,
    local_port: u16,
    timeout_ms: u64,
) -> Result<SocketAddr, StunError> {
    let socket = match server.ip() {
        IpAddr::V4(_) => UdpSocket::bind((Ipv4Addr::UNSPECIFIED, local_port)).await?,
        IpAddr::V6(_) => UdpSocket::bind((IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), local_port)).await?,
    };
    let mut tx_id = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut tx_id);

    let mut req = Vec::with_capacity(20);
    req.extend_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
    req.extend_from_slice(&0u16.to_be_bytes());
    req.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    req.extend_from_slice(&tx_id);

    socket.send_to(&req, server).await?;
    let mut buf = [0u8; 1024];
    let (len, _) = timeout(Duration::from_millis(timeout_ms), socket.recv_from(&mut buf))
        .await
        .map_err(|_| StunError::NoResponses)??;
    parse_stun_response(&buf[..len], &tx_id)
}

fn parse_stun_response(buf: &[u8], tx_id: &[u8; 12]) -> Result<SocketAddr, StunError> {
    if buf.len() < 20 {
        return Err(StunError::InvalidResponse);
    }
    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    let msg_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if msg_type != STUN_BINDING_RESPONSE || cookie != STUN_MAGIC_COOKIE {
        return Err(StunError::InvalidResponse);
    }
    if &buf[8..20] != tx_id {
        return Err(StunError::InvalidResponse);
    }

    let mut offset = 20usize;
    let end = 20 + msg_len.min(buf.len().saturating_sub(20));
    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let attr_len = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start + attr_len;
        if value_end > buf.len() {
            break;
        }
        if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS || attr_type == STUN_ATTR_MAPPED_ADDRESS {
            if let Ok(addr) = parse_mapped_address(&buf[value_start..value_end], attr_type, tx_id) {
                return Ok(addr);
            }
        }
        let padded = (attr_len + 3) & !3;
        offset = value_start + padded;
    }
    Err(StunError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::task::JoinHandle;

    async fn spawn_mock_stun(addr: SocketAddr, mapped: SocketAddr) -> JoinHandle<()> {
        tokio::spawn(async move {
            let socket = UdpSocket::bind(addr).await.expect("bind stun");
            let mut buf = [0u8; 1024];
            let Ok((len, peer)) = socket.recv_from(&mut buf).await else {
                return;
            };
            if len < 20 {
                return;
            }
            let tx_id: [u8; 12] = buf[8..20].try_into().unwrap();
            let response = build_stun_response(&tx_id, mapped);
            let _ = socket.send_to(&response, peer).await;
        })
    }

    fn build_stun_response(tx_id: &[u8; 12], mapped: SocketAddr) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        out.extend_from_slice(tx_id);

        match mapped {
            SocketAddr::V4(addr) => {
                let port = addr.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
                let ip = u32::from(*addr.ip()) ^ STUN_MAGIC_COOKIE;
                let mut attr = Vec::with_capacity(12);
                attr.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
                attr.extend_from_slice(&8u16.to_be_bytes());
                attr.push(0);
                attr.push(0x01);
                attr.extend_from_slice(&port.to_be_bytes());
                attr.extend_from_slice(&ip.to_be_bytes());
                let len = attr.len() as u16;
                out[2..4].copy_from_slice(&len.to_be_bytes());
                out.extend_from_slice(&attr);
            }
            SocketAddr::V6(addr) => {
                let port = addr.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
                let mut ip = addr.ip().octets();
                let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
                for i in 0..4 {
                    ip[i] ^= cookie[i];
                }
                for i in 0..12 {
                    ip[4 + i] ^= tx_id[i];
                }
                let mut attr = Vec::with_capacity(24);
                attr.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
                attr.extend_from_slice(&20u16.to_be_bytes());
                attr.push(0);
                attr.push(0x02);
                attr.extend_from_slice(&port.to_be_bytes());
                attr.extend_from_slice(&ip);
                let len = attr.len() as u16;
                out[2..4].copy_from_slice(&len.to_be_bytes());
                out.extend_from_slice(&attr);
            }
        }
        out
    }

    #[tokio::test]
    async fn stun_binding_returns_mapped_addr() {
        let stun_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 34878);
        let mapped = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)), 54321);
        let _handle = spawn_mock_stun(stun_addr, mapped).await;

        let addr = stun_binding_request(stun_addr, 0, 1000).await.unwrap();
        assert_eq!(addr, mapped);
    }

    #[test]
    fn local_candidates_exclude_loopback() {
        let list = gather_local_candidates(9999);
        for addr in list {
            assert!(!addr.ip().is_loopback());
        }
    }
}

fn parse_mapped_address(
    value: &[u8],
    attr_type: u16,
    tx_id: &[u8; 12],
) -> Result<SocketAddr, StunError> {
    if value.len() < 4 {
        return Err(StunError::InvalidResponse);
    }
    let family = value[1];
    let port = u16::from_be_bytes([value[2], value[3]]);
    let port = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
        port ^ ((STUN_MAGIC_COOKIE >> 16) as u16)
    } else {
        port
    };
    match family {
        0x01 => {
            if value.len() < 8 {
                return Err(StunError::InvalidResponse);
            }
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&value[4..8]);
            if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
                for i in 0..4 {
                    ip[i] ^= cookie[i];
                }
            }
            Ok(SocketAddr::new(IpAddr::V4(ip.into()), port))
        }
        0x02 => {
            if value.len() < 20 {
                return Err(StunError::InvalidResponse);
            }
            let mut ip = [0u8; 16];
            ip.copy_from_slice(&value[4..20]);
            if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                let mut xor = [0u8; 16];
                xor[..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
                xor[4..].copy_from_slice(tx_id);
                for i in 0..16 {
                    ip[i] ^= xor[i];
                }
            }
            Ok(SocketAddr::new(IpAddr::V6(ip.into()), port))
        }
        _ => Err(StunError::InvalidResponse),
    }
}
