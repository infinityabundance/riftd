use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use rift_core::{ChannelId, PeerId};

const SERVICE_TYPE: &str = "_rift._udp.local.";

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub channel_name: String,
    pub password: Option<String>,
    pub peer_id: PeerId,
    pub listen_port: u16,
}

impl DiscoveryConfig {
    pub fn channel_id(&self) -> ChannelId {
        ChannelId::from_channel(&self.channel_name, self.password.as_deref())
    }
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addr: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("mdns error: {0}")]
    Mdns(#[from] mdns_sd::Error),
    #[error("missing peer info in mDNS record")]
    MissingPeerInfo,
    #[error("invalid peer id")]
    InvalidPeerId,
}

pub struct MdnsHandle {
    _daemon: Arc<ServiceDaemon>,
    _service: ServiceInfo,
}

impl MdnsHandle {
    pub fn new(daemon: Arc<ServiceDaemon>, service: ServiceInfo) -> Self {
        Self {
            _daemon: daemon,
            _service: service,
        }
    }
}

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

struct MdnsStream {
    _daemon: ServiceDaemon,
    inner: ReceiverStream<PeerInfo>,
}

impl Stream for MdnsStream {
    type Item = PeerInfo;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

fn local_ipv4_addrs() -> Result<Vec<IpAddr>, DiscoveryError> {
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
