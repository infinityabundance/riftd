//! Peer discovery helpers (mDNS + optional DHT).
//!
//! This module provides LAN discovery via mDNS and optional internet discovery
//! via the DHT. It exposes async helpers to start advertisements and streams of
//! discovered peers.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use rift_core::{ChannelId, PeerId};
use rift_dht::{DhtConfig, DhtHandle, PeerEndpointInfo};

/// mDNS service type used for LAN discovery.
const SERVICE_TYPE: &str = "_rift._udp.local.";

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Channel name used to compute the discovery key.
    pub channel_name: String,
    /// Optional channel password.
    pub password: Option<String>,
    /// Local peer id to advertise.
    pub peer_id: PeerId,
    /// Local UDP listen port.
    pub listen_port: u16,
}

impl DiscoveryConfig {
    /// Derive the channel id used for discovery filtering.
    pub fn channel_id(&self) -> ChannelId {
        ChannelId::from_channel(&self.channel_name, self.password.as_deref())
    }
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer id discovered on the network.
    pub peer_id: PeerId,
    /// Primary socket address for the peer.
    pub addr: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// mDNS library errors.
    #[error("mdns error: {0}")]
    Mdns(#[from] mdns_sd::Error),
    /// Missing expected metadata in mDNS records.
    #[error("missing peer info in mDNS record")]
    MissingPeerInfo,
    /// Peer id could not be parsed.
    #[error("invalid peer id")]
    InvalidPeerId,
    /// DHT errors are wrapped as strings.
    #[error("dht error: {0}")]
    Dht(String),
}

#[derive(Debug, Clone)]
pub enum DiscoveryMode {
    /// Local LAN discovery only.
    Lan,
    /// Internet discovery via DHT.
    Dht(DhtConfig),
}

/// Start a DHT instance for internet discovery.
pub async fn start_dht(config: DhtConfig) -> Result<DhtHandle, DiscoveryError> {
    DhtHandle::new(config)
        .await
        .map_err(|e| DiscoveryError::Dht(e.to_string()))
}

/// Announce a peer in the DHT for the given channel.
pub async fn dht_announce(
    handle: &DhtHandle,
    channel_id: ChannelId,
    info: PeerEndpointInfo,
) -> Result<(), DiscoveryError> {
    handle
        .announce(channel_id, info)
        .await
        .map_err(|e| DiscoveryError::Dht(e.to_string()))
}

/// Lookup peers in the DHT for the given channel.
pub async fn dht_lookup(
    handle: &DhtHandle,
    channel_id: ChannelId,
) -> Result<Vec<PeerEndpointInfo>, DiscoveryError> {
    handle
        .lookup(channel_id)
        .await
        .map_err(|e| DiscoveryError::Dht(e.to_string()))
}

/// Keeps the mDNS daemon and service registration alive.
pub struct MdnsHandle {
    _daemon: Arc<ServiceDaemon>,
    _service: ServiceInfo,
}

impl MdnsHandle {
    /// Construct a handle from daemon + service info.
    pub fn new(daemon: Arc<ServiceDaemon>, service: ServiceInfo) -> Self {
        Self {
            _daemon: daemon,
            _service: service,
        }
    }
}

/// Publish this peer's presence on the LAN via mDNS.
pub fn start_mdns_advertisement(config: DiscoveryConfig) -> Result<MdnsHandle, DiscoveryError> {
    let daemon = Arc::new(ServiceDaemon::new()?);
    let channel_id = config.channel_id();
    let channel_hex = hex::encode(channel_id.0);
    let peer_hex = hex::encode(config.peer_id.0);

    let instance_name = format!("rift-{}", &peer_hex[..8]);
    let host_name = format!("{}.local.", instance_name);

    let props = [("channel", channel_hex.as_str()), ("peer", peer_hex.as_str())];
    let addrs = local_ipv4_addrs()
        .unwrap_or_else(|_| vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &host_name,
        addrs.as_slice(),
        config.listen_port,
        &props[..],
    )?;
    daemon.register(service.clone())?;

    Ok(MdnsHandle::new(daemon, service))
}

/// Start browsing for peers in the same channel on the LAN.
pub fn discover_peers(
    config: DiscoveryConfig,
) -> Result<impl Stream<Item = PeerInfo>, DiscoveryError> {
    let daemon = ServiceDaemon::new()?;
    let channel_hex = hex::encode(config.channel_id().0);
    let (tx, rx) = mpsc::channel(64);

    let receiver = daemon.browse(SERVICE_TYPE)?;
    std::thread::spawn(move || {
        for event in receiver {
            if let ServiceEvent::ServiceResolved(info) = event {
                if let Some(peer) = peer_info_from_service(&info, &channel_hex) {
                    let _ = tx.blocking_send(peer);
                }
            }
        }
    });

    Ok(MdnsStream {
        _daemon: daemon,
        inner: ReceiverStream::new(rx),
    })
}

/// Extract peer metadata from an mDNS service record.
fn peer_info_from_service(info: &ServiceInfo, channel_hex: &str) -> Option<PeerInfo> {
    let channel = info.get_property_val_str("channel")?;
    if channel != channel_hex {
        return None;
    }
    let peer_hex = info.get_property_val_str("peer")?;
    let peer_bytes = hex::decode(peer_hex).ok()?;
    if peer_bytes.len() != 32 {
        return None;
    }
    let mut peer_id = [0u8; 32];
    peer_id.copy_from_slice(&peer_bytes);

    let port = info.get_port();
    let addr = info
        .get_addresses()
        .iter()
        .find_map(|addr| {
            let sock = SocketAddr::new(*addr, port);
            Some(sock)
        })?;

    Some(PeerInfo {
        peer_id: PeerId(peer_id),
        addr,
    })
}

/// Stream wrapper that keeps the mDNS daemon alive.
struct MdnsStream {
    _daemon: ServiceDaemon,
    inner: ReceiverStream<PeerInfo>,
}

impl Stream for MdnsStream {
    type Item = PeerInfo;

    /// Delegate polling to the underlying receiver stream.
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Enumerate local IPv4 addresses for mDNS advertisement.
pub fn local_ipv4_addrs() -> Result<Vec<IpAddr>, DiscoveryError> {
    let mut addrs = Vec::new();
    let interfaces = if_addrs::get_if_addrs()
        .map_err(|e| DiscoveryError::Mdns(mdns_sd::Error::Msg(e.to_string())))?;
    for iface in interfaces {
        if let IpAddr::V4(ip) = iface.ip() {
            if !ip.is_unspecified() {
                addrs.push(IpAddr::V4(ip));
            }
        }
    }
    if addrs.is_empty() {
        addrs.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_deterministic() {
        let config1 = DiscoveryConfig {
            channel_name: "test-channel".to_string(),
            password: None,
            peer_id: PeerId([0u8; 32]),
            listen_port: 9000,
        };

        let config2 = DiscoveryConfig {
            channel_name: "test-channel".to_string(),
            password: None,
            peer_id: PeerId([1u8; 32]), // Different peer id
            listen_port: 9001,           // Different port
        };

        // Same channel name without password should produce same channel ID
        assert_eq!(config1.channel_id(), config2.channel_id());
    }

    #[test]
    fn channel_id_with_password() {
        let config_no_pass = DiscoveryConfig {
            channel_name: "test-channel".to_string(),
            password: None,
            peer_id: PeerId([0u8; 32]),
            listen_port: 9000,
        };

        let config_with_pass = DiscoveryConfig {
            channel_name: "test-channel".to_string(),
            password: Some("secret".to_string()),
            peer_id: PeerId([0u8; 32]),
            listen_port: 9000,
        };

        // Different passwords should produce different channel IDs
        assert_ne!(config_no_pass.channel_id(), config_with_pass.channel_id());
    }

    #[test]
    fn channel_id_different_names() {
        let config1 = DiscoveryConfig {
            channel_name: "channel-a".to_string(),
            password: None,
            peer_id: PeerId([0u8; 32]),
            listen_port: 9000,
        };

        let config2 = DiscoveryConfig {
            channel_name: "channel-b".to_string(),
            password: None,
            peer_id: PeerId([0u8; 32]),
            listen_port: 9000,
        };

        // Different channel names should produce different channel IDs
        assert_ne!(config1.channel_id(), config2.channel_id());
    }

    #[test]
    fn local_addrs_returns_something() {
        let addrs = local_ipv4_addrs().unwrap();
        // Should always return at least one address (even if just localhost)
        assert!(!addrs.is_empty());
    }

    #[test]
    fn local_addrs_are_ipv4() {
        let addrs = local_ipv4_addrs().unwrap();
        for addr in addrs {
            assert!(matches!(addr, IpAddr::V4(_)));
        }
    }

    #[test]
    fn peer_info_construction() {
        let peer = PeerInfo {
            peer_id: PeerId([42u8; 32]),
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 9000),
        };

        assert_eq!(peer.peer_id.0, [42u8; 32]);
        assert_eq!(peer.addr.port(), 9000);
    }

    #[test]
    fn discovery_error_display() {
        let err = DiscoveryError::MissingPeerInfo;
        assert_eq!(format!("{}", err), "missing peer info in mDNS record");

        let err = DiscoveryError::InvalidPeerId;
        assert_eq!(format!("{}", err), "invalid peer id");
    }
}
