# rift-torrent

<p align="center">
  <a href="https://crates.io/crates/rift-torrent"><img src="https://img.shields.io/crates/v/rift-torrent.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-torrent"><img src="https://docs.rs/rift-torrent/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

SRT-based torrent peer discovery for zero-infrastructure P2P.

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP with Predictive Rendezvous.

## What's in this crate?

- **Zero-Infrastructure Discovery**: Use Semantic Rendezvous Tokens (SRTs) derived from infohash instead of trackers or DHT bootstrap nodes
- **BitTorrent Compatible**: Parse and create standard .torrent files with optional SRT extensions
- **Magnet URI Support**: Parse magnet URIs with `xs=riftd-srt://` parameter
- **Deterministic Rendezvous**: Peers independently derive identical schedules from the infohash alone
- **Fallback Support**: Graceful degradation to traditional tracker/DHT when needed

## How It Works

Instead of announcing to a tracker or bootstrapping DHT nodes, peers derive a Semantic Rendezvous Token (SRT) from the torrent infohash:

```
infohash → BLAKE3 → space_id (namespace isolation)
infohash + t0 → BLAKE3 → seed (schedule derivation)
```

Both peers compute identical rendezvous schedules and can discover each other without any central infrastructure.

## Usage

```rust
use rift_torrent::{MagnetUri, SwarmSrt, SwarmDiscovery, DiscoveryConfig, InfoHash};
use rift_rndzv::PeerId;

// Parse a magnet URI
let magnet = MagnetUri::parse(
    "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Example"
)?;

// Derive SRT from infohash (zero-infrastructure discovery)
let srt = SwarmSrt::from_infohash(*magnet.primary_hash());

// Get the SRT URI for sharing
let srt_uri = srt.to_uri()?;
println!("SRT: {}", srt_uri);

// Create extended magnet with SRT
let extended_magnet = magnet.with_srt(srt_uri);
println!("Magnet: {}", extended_magnet.to_uri());
```

### Parsing .torrent Files

```rust
use rift_torrent::{parse_torrent, SwarmSrt};

let torrent_data = std::fs::read("example.torrent")?;
let meta = parse_torrent(&torrent_data)?;

println!("Name: {}", meta.name);
println!("Size: {} bytes", meta.total_length);
println!("Pieces: {}", meta.piece_count());

// Derive SRT for peer discovery
let srt = SwarmSrt::from_infohash(meta.info_hash);
```

### Peer Discovery

```rust
use rift_torrent::{SwarmSrt, SwarmDiscovery, SwarmPeer, DiscoveryConfig, InfoHash};
use rift_rndzv::PeerId;

let info_hash = InfoHash::Sha1([0xAB; 20]);
let srt = SwarmSrt::from_infohash(info_hash);
let local_peer = PeerId([0u8; 32]);

let mut discovery = SwarmDiscovery::new(srt, local_peer, DiscoveryConfig::default());

// Add discovered peers
let peer = SwarmPeer::new(PeerId([1u8; 32]), vec!["192.168.1.100:6881".parse()?]);
discovery.add_peer(peer);

// Check if SRT needs refreshing
if discovery.needs_refresh() {
    discovery.refresh_srt();
}
```

## Magnet URI Extension

Standard magnet URIs can include an SRT in the `xs` (exact source) parameter:

```
magnet:?xt=urn:btih:HASH&dn=NAME&xs=riftd-srt://v1?space=...&seed=...
```

This allows zero-infrastructure peer discovery while remaining compatible with existing BitTorrent clients (which will ignore the unknown `xs` parameter).

## Discovery Modes

| Mode | Description |
|------|-------------|
| `SrtOnly` | Pure zero-infrastructure, SRT rendezvous only |
| `SrtThenDht` | Try SRT first, fall back to DHT (default) |
| `SrtAndDht` | Use SRT and DHT in parallel |
| `Traditional` | Traditional tracker/DHT only |

## Related Crates

| Crate | Description |
|-------|-------------|
| [`rift-rndzv`](https://crates.io/crates/rift-rndzv) | Predictive Rendezvous networking layer |
| [`rift-core`](https://crates.io/crates/rift-core) | Core types, identity, cryptography |
| [`rift-dht`](https://crates.io/crates/rift-dht) | Distributed hash table for peer discovery |

## Citation

If you use this work in academic research, please cite:

> Riaan de Beer. *Predictive Rendezvous: Time–Intent–Deterministic Peer Coordination Without Infrastructure.* 2026. DOI: [10.5281/zenodo.18528430](https://doi.org/10.5281/zenodo.18528430)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
