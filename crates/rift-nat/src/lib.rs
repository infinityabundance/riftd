use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};

use rift_core::PeerId;

#[derive(Debug, Clone)]
pub struct NatConfig {
    pub local_ports: Vec<u16>,
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

const PUNCH_SYN: &[u8] = b"RIFT_PUNCH";
const PUNCH_ACK: &[u8] = b"RIFT_ACK";

pub async fn attempt_hole_punch(
    nat_cfg: &NatConfig,
    peer: &PeerEndpoint,
) -> Result<(UdpSocket, SocketAddr), HolePunchError> {
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
        return Err(HolePunchError::NoLocalPorts);
    }

    let target_addrs = build_target_addrs(peer);
    if target_addrs.is_empty() {
        return Err(HolePunchError::NoRemoteAddrs);
    }

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
            let mut tick = interval(Duration::from_millis(200));
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

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    match result {
        Ok(Some((socket, addr))) => Ok((socket, addr)),
        _ => Err(HolePunchError::Timeout),
    }
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
