//! rift-torrent: SRT-based torrent peer discovery for zero-infrastructure P2P.
//!
//! This crate provides BitTorrent-compatible metadata parsing with Predictive
//! Rendezvous (SRT) extensions for zero-infrastructure peer discovery.
//!
//! ## Key Features
//!
//! - **Zero-Infrastructure Discovery**: Use SRTs derived from infohash instead of
//!   trackers or DHT bootstrap nodes
//! - **BitTorrent Compatible**: Parse and create standard .torrent files with
//!   optional SRT extensions
//! - **Magnet URI Support**: Parse magnet URIs with `xs=riftd-srt://` parameter
//! - **Deterministic Rendezvous**: Peers independently derive identical schedules
//!   from the infohash alone
//!
//! ## How It Works
//!
//! Instead of announcing to a tracker or bootstrapping DHT nodes, peers derive
//! a Semantic Rendezvous Token (SRT) from the torrent infohash:
//!
//! ```text
//! infohash → BLAKE3 → space_id (namespace isolation)
//! infohash + t0 → BLAKE3 → seed (schedule derivation)
//! ```
//!
//! Both peers compute identical rendezvous schedules and can discover each other
//! without any central infrastructure.
//!
//! ## Example
//!
//! ```rust,no_run
//! use rift_torrent::{MagnetUri, SwarmSrt, SwarmDiscovery, DiscoveryConfig, InfoHash};
//! use rift_rndzv::PeerId;
//!
//! // Parse a magnet URI with SRT extension
//! let magnet = MagnetUri::parse(
//!     "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&xs=riftd-srt://v1..."
//! ).unwrap();
//!
//! // Or derive SRT from just an infohash
//! let srt = SwarmSrt::from_infohash(*magnet.primary_hash());
//!
//! // Create a discovery instance
//! let local_peer = PeerId([0u8; 32]); // Your peer ID
//! let discovery = SwarmDiscovery::new(srt, local_peer, DiscoveryConfig::default());
//!
//! // Get the SRT URI for sharing
//! let uri = discovery.srt_uri().unwrap();
//! println!("Share this SRT: {}", uri);
//! ```
//!
//! ## Magnet URI Extension
//!
//! Standard magnet URIs can include an SRT in the `xs` (exact source) parameter:
//!
//! ```text
//! magnet:?xt=urn:btih:HASH&dn=NAME&xs=riftd-srt://v1?space=...&seed=...
//! ```
//!
//! ## Fallback Strategy
//!
//! When SRT discovery fails or times out, the crate supports fallback to
//! traditional methods:
//!
//! 1. **SRT Rendezvous** (primary) - Zero-infrastructure
//! 2. **DHT Hints** (hybrid) - Use DHT for candidate addresses
//! 3. **DHT Bootstrap** (fallback) - Standard Kademlia lookup
//! 4. **Tracker Announce** (fallback) - HTTP/UDP trackers

pub mod bencode;
pub mod error;
pub mod magnet;
pub mod meta;
pub mod peer;
pub mod srt;

// Re-exports for convenience
pub use error::TorrentError;
pub use magnet::MagnetUri;
pub use meta::{FileInfo, InfoHash, PieceHash, SrtExtension, TorrentMeta};
pub use peer::{DiscoveryConfig, DiscoveryMode, PeerExchange, PexPeer, SwarmDiscovery, SwarmPeer};
pub use srt::{derive_seed, derive_space_id, SwarmSrt};

// Re-export bencode utilities
pub use bencode::{decode as decode_bencode, encode as encode_bencode, parse_torrent, BValue};

// Re-export commonly used rift-rndzv types
pub use rift_rndzv::{PeerId, RendezvousSpaceId, TimeModel};
