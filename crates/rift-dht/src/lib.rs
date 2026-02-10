//! Libp2p-based DHT integration for internet discovery.
//!
//! This module maintains a lightweight Kademlia DHT used to announce and
//! discover peer endpoints keyed by channel id. It exposes a simple async
//! handle (`DhtHandle`) to announce or lookup peers.

use std::collections::HashMap;
use std::net::SocketAddr;

use anyhow::Result;
use libp2p::core::upgrade;
use libp2p::identify::{Behaviour as Identify, Config as IdentifyConfig, Event as IdentifyEvent};
use libp2p::kad::{
    store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent, GetProvidersOk,
    GetRecordOk, PutRecordOk, QueryId, QueryResult, Quorum, Record, RecordKey,
};
use libp2p::multiaddr::Protocol;
use libp2p::noise;
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{tcp, yamux, Multiaddr, PeerId, Transport};
use rift_core::{ChannelId, PeerId as RiftPeerId};
use rift_metrics as metrics;
use tracing::debug;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use futures::StreamExt;

#[derive(Debug, Clone)]
pub struct DhtConfig {
    /// Bootstrap node socket addresses.
    pub bootstrap_nodes: Vec<SocketAddr>,
    /// Local listen address for the DHT.
    pub listen_addr: SocketAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerEndpointInfo {
    /// Rift peer id.
    pub peer_id: RiftPeerId,
    /// Known socket addresses for the peer.
    pub addrs: Vec<SocketAddr>,
}

#[derive(Debug, thiserror::Error)]
pub enum DhtError {
    /// Transport/IO errors from the libp2p stack.
    #[error("transport error: {0}")]
    Transport(String),
    /// DHT query errors.
    #[error("dht error: {0}")]
    Dht(String),
}

#[derive(Clone)]
pub struct DhtHandle {
    /// Command channel for interacting with the background swarm task.
    cmd_tx: mpsc::Sender<Command>,
}

enum Command {
    /// Announce peer info for a channel and become a provider.
    Announce {
        key: ChannelId,
        info: PeerEndpointInfo,
        resp: oneshot::Sender<Result<(), DhtError>>,
    },
    /// Lookup peers providing a channel.
    Lookup {
        key: ChannelId,
        resp: oneshot::Sender<Result<Vec<PeerEndpointInfo>, DhtError>>,
    },
}

/// Combined network behaviours used by the libp2p swarm.
#[derive(NetworkBehaviour)]
struct Behaviour {
    kademlia: Kademlia<MemoryStore>,
    identify: Identify,
}

/// Tracks in-flight lookups to aggregate multiple record results.
struct LookupState {
    channel: ChannelId,
    pending: usize,
    results: Vec<PeerEndpointInfo>,
    resp: oneshot::Sender<Result<Vec<PeerEndpointInfo>, DhtError>>,
}

/// Internal classifier to connect query ids with lookup stages.
enum LookupKind {
    Providers { lookup_id: u64 },
    Record { lookup_id: u64 },
}

impl DhtHandle {
    /// Spawn the DHT swarm and return a handle for interaction.
    pub async fn new(config: DhtConfig) -> Result<DhtHandle, DhtError> {
        let local_key = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_key).map_err(|e| DhtError::Transport(e.to_string()))?)
            .multiplex(yamux::Config::default())
            .boxed();

        let store = MemoryStore::new(local_peer_id);
        let mut kademlia = Kademlia::new(local_peer_id, store);
        kademlia.set_mode(Some(libp2p::kad::Mode::Server));

        let identify = Identify::new(IdentifyConfig::new(
            "rift-dht/1.0.0".to_string(),
            local_key.public(),
        ));

        let behaviour = Behaviour { kademlia, identify };
        let mut swarm = Swarm::new(
            transport,
            behaviour,
            local_peer_id,
            libp2p::swarm::Config::with_tokio_executor(),
        );

        let listen_addr = socket_to_multiaddr(config.listen_addr);
        swarm
            .listen_on(listen_addr)
            .map_err(|e| DhtError::Transport(e.to_string()))?;

        let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
        let mut pending_put: HashMap<QueryId, oneshot::Sender<Result<(), DhtError>>> = HashMap::new();
        let mut pending_lookup: HashMap<QueryId, LookupKind> = HashMap::new();
        let mut lookups: HashMap<u64, LookupState> = HashMap::new();
        let mut next_lookup_id = 1u64;

        for addr in config.bootstrap_nodes {
            let multi = socket_to_multiaddr(addr);
            let _ = swarm.dial(multi);
        }

        // Background task: drive swarm and answer requests.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => match cmd {
                        Command::Announce { key, info, resp } => {
                            let channel_key = channel_key(key);
                            let record_key = peer_record_key(key, info.peer_id);
                            let value = bincode::serialize(&info).unwrap_or_default();
                            let record = Record { key: record_key, value, publisher: None, expires: None };
                            let qid = swarm.behaviour_mut().kademlia.put_record(record, Quorum::One);
                            if let Ok(qid) = qid {
                                pending_put.insert(qid, resp);
                            } else {
                                let _ = resp.send(Err(DhtError::Dht("put record failed".to_string())));
                            }
                            let _ = swarm.behaviour_mut().kademlia.start_providing(channel_key);
                        }
                        Command::Lookup { key, resp } => {
                            let lookup_id = next_lookup_id;
                            next_lookup_id += 1;
                            let qid = swarm.behaviour_mut().kademlia.get_providers(channel_key(key));
                            pending_lookup.insert(qid, LookupKind::Providers { lookup_id });
                            lookups.insert(lookup_id, LookupState { channel: key, pending: 0, results: Vec::new(), resp });
                        }
                    },
                    event = swarm.select_next_some() => match event {
                        SwarmEvent::Behaviour(BehaviourEvent::Identify(IdentifyEvent::Received { peer_id, info, .. })) => {
                            for addr in info.listen_addrs {
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            }
                            let _ = swarm.behaviour_mut().kademlia.bootstrap();
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Kademlia(event)) => {
                            if let KademliaEvent::OutboundQueryProgressed { id, result, .. } = event {
                                match result {
                                    QueryResult::PutRecord(Ok(PutRecordOk { .. })) => {
                                        if let Some(resp) = pending_put.remove(&id) {
                                            let _ = resp.send(Ok(()));
                                        }
                                    }
                                    QueryResult::PutRecord(Err(err)) => {
                                        if let Some(resp) = pending_put.remove(&id) {
                                            let _ = resp.send(Err(DhtError::Dht(err.to_string())));
                                        }
                                    }
                                    QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders { providers, .. })) => {
                                        if let Some(LookupKind::Providers { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.get_mut(&lookup_id) {
                                                if providers.is_empty() {
                                                    let state = lookups.remove(&lookup_id).unwrap();
                                                    let _ = state.resp.send(Ok(state.results));
                                                } else {
                                                    state.pending = providers.len();
                                                    for provider in providers {
                                                        let record_key = peer_record_key_from_peer(state.channel, provider);
                                                        let qid = swarm.behaviour_mut().kademlia.get_record(record_key);
                                                        pending_lookup.insert(qid, LookupKind::Record { lookup_id });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    QueryResult::GetProviders(Ok(GetProvidersOk::FinishedWithNoAdditionalRecord { .. })) => {
                                        if let Some(LookupKind::Providers { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.remove(&lookup_id) {
                                                let _ = state.resp.send(Ok(state.results));
                                            }
                                        }
                                    }
                                    QueryResult::GetProviders(Err(err)) => {
                                        if let Some(LookupKind::Providers { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.remove(&lookup_id) {
                                                let _ = state.resp.send(Err(DhtError::Dht(err.to_string())));
                                            }
                                        }
                                    }
                                    QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(record))) => {
                                        if let Some(LookupKind::Record { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.get_mut(&lookup_id) {
                                                if let Ok(info) = bincode::deserialize::<PeerEndpointInfo>(&record.record.value) {
                                                    state.results.push(info);
                                                }
                                                if state.pending > 0 {
                                                    state.pending -= 1;
                                                }
                                                if state.pending == 0 {
                                                    let state = lookups.remove(&lookup_id).unwrap();
                                                    let _ = state.resp.send(Ok(state.results));
                                                }
                                            }
                                        }
                                    }
                                    QueryResult::GetRecord(Ok(GetRecordOk::FinishedWithNoAdditionalRecord { .. })) => {
                                        if let Some(LookupKind::Record { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.get_mut(&lookup_id) {
                                                if state.pending > 0 {
                                                    state.pending -= 1;
                                                }
                                                if state.pending == 0 {
                                                    let state = lookups.remove(&lookup_id).unwrap();
                                                    let _ = state.resp.send(Ok(state.results));
                                                }
                                            }
                                        }
                                    }
                                    QueryResult::GetRecord(Err(err)) => {
                                        if let Some(LookupKind::Record { lookup_id }) = pending_lookup.remove(&id) {
                                            if let Some(state) = lookups.get_mut(&lookup_id) {
                                                if state.pending > 0 {
                                                    state.pending -= 1;
                                                }
                                                if state.pending == 0 {
                                                    let state = lookups.remove(&lookup_id).unwrap();
                                                    let _ = state.resp.send(Err(DhtError::Dht(err.to_string())));
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        SwarmEvent::NewListenAddr { .. } => {}
                        _ => {}
                    }
                }
            }
        });

        metrics::inc_counter("rift_dht_started", &[]);
        Ok(DhtHandle { cmd_tx })
    }

    /// Announce the current peer for a channel.
    pub async fn announce(&self, key: ChannelId, info: PeerEndpointInfo) -> Result<(), DhtError> {
        metrics::inc_counter("rift_dht_announce", &[]);
        debug!(channel = %key.to_hex(), "dht announce");
        let (tx, rx) = oneshot::channel();
        let cmd = Command::Announce { key, info, resp: tx };
        let _ = self.cmd_tx.send(cmd).await;
        rx.await.unwrap_or(Err(DhtError::Dht("announce failed".to_string())))
    }

    /// Lookup all peers advertising a given channel.
    pub async fn lookup(&self, key: ChannelId) -> Result<Vec<PeerEndpointInfo>, DhtError> {
        metrics::inc_counter("rift_dht_lookup", &[]);
        debug!(channel = %key.to_hex(), "dht lookup");
        let (tx, rx) = oneshot::channel();
        let cmd = Command::Lookup { key, resp: tx };
        let _ = self.cmd_tx.send(cmd).await;
        rx.await.unwrap_or(Err(DhtError::Dht("lookup failed".to_string())))
    }
}

/// Convert a socket address to a multiaddr used by libp2p.
fn socket_to_multiaddr(addr: SocketAddr) -> Multiaddr {
    match addr {
        SocketAddr::V4(v4) => Multiaddr::empty()
            .with(Protocol::Ip4(*v4.ip()))
            .with(Protocol::Tcp(v4.port())),
        SocketAddr::V6(v6) => Multiaddr::empty()
            .with(Protocol::Ip6(*v6.ip()))
            .with(Protocol::Tcp(v6.port())),
    }
}

/// Record key used to advertise a channel.
fn channel_key(channel: ChannelId) -> RecordKey {
    RecordKey::new(&channel.0)
}

/// Record key used to store a peer record within a channel.
fn peer_record_key(channel: ChannelId, peer_id: RiftPeerId) -> RecordKey {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&channel.0);
    bytes.extend_from_slice(&peer_id.0);
    RecordKey::new(&bytes)
}

/// Record key used when we only have a libp2p peer id.
fn peer_record_key_from_peer(channel: ChannelId, peer_id: PeerId) -> RecordKey {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&channel.0);
    bytes.extend_from_slice(peer_id.to_bytes().as_ref());
    RecordKey::new(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn peer_endpoint_info_serialization_roundtrip() {
        let info = PeerEndpointInfo {
            peer_id: RiftPeerId([42u8; 32]),
            addrs: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 9000),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 9001),
            ],
        };

        let serialized = bincode::serialize(&info).unwrap();
        let deserialized: PeerEndpointInfo = bincode::deserialize(&serialized).unwrap();

        assert_eq!(info.peer_id.0, deserialized.peer_id.0);
        assert_eq!(info.addrs, deserialized.addrs);
    }

    #[test]
    fn peer_endpoint_info_empty_addrs() {
        let info = PeerEndpointInfo {
            peer_id: RiftPeerId([0u8; 32]),
            addrs: vec![],
        };

        let serialized = bincode::serialize(&info).unwrap();
        let deserialized: PeerEndpointInfo = bincode::deserialize(&serialized).unwrap();

        assert_eq!(info.addrs.len(), 0);
        assert_eq!(deserialized.addrs.len(), 0);
    }

    #[test]
    fn dht_config_construction() {
        let config = DhtConfig {
            bootstrap_nodes: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 4001),
            ],
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        };

        assert_eq!(config.bootstrap_nodes.len(), 1);
        assert_eq!(config.listen_addr.port(), 0);
    }

    #[test]
    fn dht_error_display() {
        let err = DhtError::Transport("connection refused".to_string());
        assert!(format!("{}", err).contains("transport error"));

        let err = DhtError::Dht("no providers".to_string());
        assert!(format!("{}", err).contains("dht error"));
    }

    #[test]
    fn socket_to_multiaddr_ipv4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4001);
        let multi = socket_to_multiaddr(addr);
        let expected = "/ip4/127.0.0.1/tcp/4001".parse::<Multiaddr>().unwrap();
        assert_eq!(multi, expected);
    }

    #[test]
    fn socket_to_multiaddr_ipv6() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 4001);
        let multi = socket_to_multiaddr(addr);
        let expected = "/ip6/::1/tcp/4001".parse::<Multiaddr>().unwrap();
        assert_eq!(multi, expected);
    }

    #[test]
    fn channel_key_deterministic() {
        let channel = ChannelId([42u8; 32]);
        let key1 = channel_key(channel);
        let key2 = channel_key(channel);
        assert_eq!(key1, key2);
    }

    #[test]
    fn channel_key_different_channels() {
        let channel1 = ChannelId([1u8; 32]);
        let channel2 = ChannelId([2u8; 32]);
        let key1 = channel_key(channel1);
        let key2 = channel_key(channel2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn peer_record_key_deterministic() {
        let channel = ChannelId([42u8; 32]);
        let peer = RiftPeerId([7u8; 32]);
        let key1 = peer_record_key(channel, peer);
        let key2 = peer_record_key(channel, peer);
        assert_eq!(key1, key2);
    }

    #[test]
    fn peer_record_key_different_peers() {
        let channel = ChannelId([42u8; 32]);
        let peer1 = RiftPeerId([1u8; 32]);
        let peer2 = RiftPeerId([2u8; 32]);
        let key1 = peer_record_key(channel, peer1);
        let key2 = peer_record_key(channel, peer2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn peer_record_key_different_channels() {
        let channel1 = ChannelId([1u8; 32]);
        let channel2 = ChannelId([2u8; 32]);
        let peer = RiftPeerId([42u8; 32]);
        let key1 = peer_record_key(channel1, peer);
        let key2 = peer_record_key(channel2, peer);
        assert_ne!(key1, key2);
    }

    use std::net::IpAddr;
}
