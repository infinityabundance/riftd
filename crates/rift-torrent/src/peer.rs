//! Peer discovery using SRT and fallback mechanisms.

use std::collections::HashSet;
use std::net::SocketAddr;

use rift_rndzv::PeerId;
use serde::{Deserialize, Serialize};

use crate::{SwarmSrt, TorrentError};

/// Peer information for a torrent swarm.
#[derive(Clone, Debug)]
pub struct SwarmPeer {
    /// Rift peer identifier.
    pub peer_id: PeerId,
    /// Known addresses for this peer.
    pub addrs: Vec<SocketAddr>,
    /// Last seen timestamp (Unix seconds).
    pub last_seen: u64,
    /// Whether this peer supports SRT.
    pub srt_capable: bool,
}

impl SwarmPeer {
    /// Create a new swarm peer.
    pub fn new(peer_id: PeerId, addrs: Vec<SocketAddr>) -> Self {
        Self {
            peer_id,
            addrs,
            last_seen: now_unix_secs(),
            srt_capable: true,
        }
    }

    /// Update the last seen timestamp.
    pub fn touch(&mut self) {
        self.last_seen = now_unix_secs();
    }

    /// Check if this peer is stale (not seen recently).
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        let now = now_unix_secs();
        now.saturating_sub(self.last_seen) > max_age_secs
    }
}

/// Discovery mode configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiscoveryMode {
    /// SRT only - pure zero-infrastructure.
    SrtOnly,
    /// SRT primary with DHT fallback.
    #[default]
    SrtThenDht,
    /// SRT and DHT in parallel.
    SrtAndDht,
    /// Traditional only (tracker/DHT).
    Traditional,
}

/// Configuration for peer discovery.
#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    /// Discovery mode.
    pub mode: DiscoveryMode,
    /// Timeout for SRT rendezvous in milliseconds.
    pub srt_timeout_ms: u64,
    /// Maximum peers to discover.
    pub max_peers: usize,
    /// Maximum age for peer staleness check (seconds).
    pub peer_max_age_secs: u64,
    /// Interval for peer exchange (seconds).
    pub pex_interval_secs: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            mode: DiscoveryMode::SrtThenDht,
            srt_timeout_ms: 30_000,
            max_peers: 50,
            peer_max_age_secs: 600, // 10 minutes
            pex_interval_secs: 60,  // 1 minute
        }
    }
}

/// Peer exchange message for swarm coordination.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerExchange {
    /// Peers added since last exchange.
    pub added: Vec<PexPeer>,
    /// Peer IDs dropped since last exchange.
    pub dropped: Vec<[u8; 32]>,
}

/// Peer entry in a PEX message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PexPeer {
    /// Peer ID bytes.
    pub peer_id: [u8; 32],
    /// Socket addresses.
    pub addrs: Vec<SocketAddr>,
    /// SRT capable flag.
    pub srt_capable: bool,
}

impl PeerExchange {
    /// Create an empty peer exchange.
    pub fn empty() -> Self {
        Self {
            added: Vec::new(),
            dropped: Vec::new(),
        }
    }

    /// Check if this exchange has any changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.dropped.is_empty()
    }

    /// Number of peers in this exchange.
    pub fn len(&self) -> usize {
        self.added.len() + self.dropped.len()
    }
}

/// Peer discovery state for a torrent swarm.
pub struct SwarmDiscovery {
    /// SRT for this swarm.
    srt: SwarmSrt,
    /// Local peer identity.
    local_peer: PeerId,
    /// Configuration.
    config: DiscoveryConfig,
    /// Known peers.
    peers: HashSet<[u8; 32]>,
    /// Peer details.
    peer_info: Vec<SwarmPeer>,
}

impl SwarmDiscovery {
    /// Create a new discovery instance.
    pub fn new(srt: SwarmSrt, local_peer: PeerId, config: DiscoveryConfig) -> Self {
        Self {
            srt,
            local_peer,
            config,
            peers: HashSet::new(),
            peer_info: Vec::new(),
        }
    }

    /// Get the SRT for this swarm.
    pub fn srt(&self) -> &SwarmSrt {
        &self.srt
    }

    /// Get the local peer ID.
    pub fn local_peer(&self) -> &PeerId {
        &self.local_peer
    }

    /// Get the discovery configuration.
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }

    /// Get the current peer count.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Check if we have enough peers.
    pub fn has_enough_peers(&self) -> bool {
        self.peers.len() >= self.config.max_peers
    }

    /// Add a discovered peer.
    pub fn add_peer(&mut self, peer: SwarmPeer) -> bool {
        if self.peers.len() >= self.config.max_peers {
            return false;
        }

        if self.peers.insert(peer.peer_id.0) {
            self.peer_info.push(peer);
            true
        } else {
            // Update existing peer
            if let Some(existing) = self.peer_info.iter_mut().find(|p| p.peer_id == peer.peer_id) {
                existing.touch();
                existing.addrs = peer.addrs;
            }
            false
        }
    }

    /// Remove a peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> bool {
        if self.peers.remove(&peer_id.0) {
            self.peer_info.retain(|p| p.peer_id != *peer_id);
            true
        } else {
            false
        }
    }

    /// Get all known peers.
    pub fn peers(&self) -> &[SwarmPeer] {
        &self.peer_info
    }

    /// Remove stale peers.
    pub fn prune_stale(&mut self) -> Vec<PeerId> {
        let max_age = self.config.peer_max_age_secs;
        let stale: Vec<_> = self
            .peer_info
            .iter()
            .filter(|p| p.is_stale(max_age))
            .map(|p| p.peer_id)
            .collect();

        for peer_id in &stale {
            self.peers.remove(&peer_id.0);
        }
        self.peer_info.retain(|p| !p.is_stale(max_age));

        stale
    }

    /// Create a peer exchange message with recent changes.
    pub fn create_pex(&self, added: &[SwarmPeer], dropped: &[PeerId]) -> PeerExchange {
        PeerExchange {
            added: added
                .iter()
                .map(|p| PexPeer {
                    peer_id: p.peer_id.0,
                    addrs: p.addrs.clone(),
                    srt_capable: p.srt_capable,
                })
                .collect(),
            dropped: dropped.iter().map(|p| p.0).collect(),
        }
    }

    /// Apply a received peer exchange.
    pub fn apply_pex(&mut self, pex: &PeerExchange) -> usize {
        let mut added_count = 0;

        for peer in &pex.added {
            let swarm_peer = SwarmPeer {
                peer_id: PeerId(peer.peer_id),
                addrs: peer.addrs.clone(),
                last_seen: now_unix_secs(),
                srt_capable: peer.srt_capable,
            };
            if self.add_peer(swarm_peer) {
                added_count += 1;
            }
        }

        for peer_id in &pex.dropped {
            self.remove_peer(&PeerId(*peer_id));
        }

        added_count
    }

    /// Refresh the SRT for a new time window.
    pub fn refresh_srt(&mut self) {
        self.srt = self.srt.refresh();
    }

    /// Check if the SRT needs refreshing.
    pub fn needs_refresh(&self) -> bool {
        !self.srt.is_valid() || self.srt.remaining_secs() < 60
    }

    /// Get the SRT URI for sharing.
    pub fn srt_uri(&self) -> Result<String, TorrentError> {
        self.srt.to_uri()
    }
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InfoHash;

    fn test_srt() -> SwarmSrt {
        SwarmSrt::from_infohash(InfoHash::Sha1([0xAB; 20]))
    }

    fn test_peer(id: u8) -> SwarmPeer {
        SwarmPeer::new(PeerId([id; 32]), vec!["127.0.0.1:6881".parse().unwrap()])
    }

    #[test]
    fn add_peer() {
        let mut discovery = SwarmDiscovery::new(
            test_srt(),
            PeerId([0x00; 32]),
            DiscoveryConfig::default(),
        );

        assert!(discovery.add_peer(test_peer(1)));
        assert_eq!(discovery.peer_count(), 1);

        // Adding same peer again should not increase count
        assert!(!discovery.add_peer(test_peer(1)));
        assert_eq!(discovery.peer_count(), 1);
    }

    #[test]
    fn remove_peer() {
        let mut discovery = SwarmDiscovery::new(
            test_srt(),
            PeerId([0x00; 32]),
            DiscoveryConfig::default(),
        );

        discovery.add_peer(test_peer(1));
        assert_eq!(discovery.peer_count(), 1);

        assert!(discovery.remove_peer(&PeerId([1; 32])));
        assert_eq!(discovery.peer_count(), 0);
    }

    #[test]
    fn max_peers_limit() {
        let config = DiscoveryConfig {
            max_peers: 2,
            ..Default::default()
        };
        let mut discovery = SwarmDiscovery::new(test_srt(), PeerId([0x00; 32]), config);

        assert!(discovery.add_peer(test_peer(1)));
        assert!(discovery.add_peer(test_peer(2)));
        assert!(!discovery.add_peer(test_peer(3))); // Should fail

        assert_eq!(discovery.peer_count(), 2);
        assert!(discovery.has_enough_peers());
    }

    #[test]
    fn pex_roundtrip() {
        let peers = vec![test_peer(1), test_peer(2)];
        let dropped = vec![PeerId([3; 32])];

        let discovery = SwarmDiscovery::new(
            test_srt(),
            PeerId([0x00; 32]),
            DiscoveryConfig::default(),
        );

        let pex = discovery.create_pex(&peers, &dropped);
        assert_eq!(pex.added.len(), 2);
        assert_eq!(pex.dropped.len(), 1);
        assert!(!pex.is_empty());
    }
}
