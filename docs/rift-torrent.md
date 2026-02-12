# rift-torrent: SRT-Based Torrent Peer Discovery

This document explains how `rift-torrent` enables zero-infrastructure torrent peer discovery using Predictive Rendezvous (SRTs).

## Overview

Traditional BitTorrent relies on centralized infrastructure for peer discovery:
- **Trackers**: HTTP/UDP servers that maintain peer lists
- **DHT**: Distributed hash table requiring bootstrap nodes
- **PEX**: Peer exchange that still needs initial peers

`rift-torrent` introduces a novel approach: **derive peer rendezvous schedules directly from the infohash**. Two peers with the same torrent can find each other without any infrastructure.

## How It Works

### The Core Insight

Given the same infohash, any two peers can independently compute:
1. A **rendezvous space** (namespace isolation)
2. A **deterministic schedule** (when and where to probe)

```
┌─────────────┐     ┌─────────────┐
│   Peer A    │     │   Peer B    │
│             │     │             │
│  infohash   │     │  infohash   │
│      ↓      │     │      ↓      │
│  BLAKE3     │     │  BLAKE3     │
│      ↓      │     │      ↓      │
│  space_id   │ === │  space_id   │  (identical)
│  seed       │ === │  seed       │  (identical)
│      ↓      │     │      ↓      │
│  schedule   │ === │  schedule   │  (identical)
│      ↓      │     │      ↓      │
│  PROBE ─────┼─────┼───► LISTEN  │
│  LISTEN ◄───┼─────┼──── PROBE   │
│      ↓      │     │      ↓      │
│  CONNECTED! │     │  CONNECTED! │
└─────────────┘     └─────────────┘
```

### Step 1: Derive Space ID

The space ID provides namespace isolation—different torrents get different rendezvous spaces:

```rust
fn derive_space_id(info_hash: &InfoHash) -> RendezvousSpaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rift-torrent-space-v1");
    hasher.update(&info_hash.to_bytes32());
    RendezvousSpaceId(*hasher.finalize().as_bytes())
}
```

### Step 2: Derive Seed

The seed drives deterministic schedule derivation:

```rust
fn derive_seed(info_hash: &InfoHash, t0: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rift-torrent-seed-v1");
    hasher.update(&info_hash.to_bytes32());
    hasher.update(&t0.to_le_bytes());
    *hasher.finalize().as_bytes()
}
```

The `t0` parameter is the time anchor—typically the torrent creation time or current time rounded to a window boundary.

### Step 3: Compute Schedule

Using the seed and time model, both peers compute identical slot sequences:

```
Time Model:
  t0 = 1700000000 (Unix seconds)
  window_secs = 300 (5 minutes)
  slot_ms = 500 (probe every 500ms)

Total slots = 300,000 ms / 500 ms = 600 slots per window
```

Each slot determines:
- **Port offsets**: Where to send/listen for probes
- **Timing**: When this slot is active
- **Burst pattern**: Probe timing within the slot

### Step 4: Rendezvous

Both peers:
1. Bind to their local port + slot-derived offset
2. Send probes to the remote's predicted port
3. Validate incoming probes against expected parameters
4. Upon match, establish a session

## Magnet URI Extension

Standard magnet URIs can include an SRT in the `xs` (exact source) parameter:

```
# Standard magnet
magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Example

# With SRT extension (zero-infrastructure discovery)
magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567
       &dn=Example
       &xs=riftd-srt://v1?space=abc...&seed=def...&t0=1700000000&tw=300&slot=500
```

Existing BitTorrent clients will ignore the unknown `xs` parameter, maintaining backwards compatibility.

## .torrent File Extension

Torrent files can embed SRT parameters in a new `srt` dictionary:

```
d
  4:info d ... e
  3:srt d
    7:version i1e
    9:t0_offset i0e
    11:window_secs i300e
    7:slot_ms i500e
  e
e
```

## Discovery Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `SrtOnly` | Pure zero-infrastructure | Maximum privacy, no fallback |
| `SrtThenDht` | Try SRT first, fall back to DHT | Default, balanced |
| `SrtAndDht` | SRT and DHT in parallel | Fastest discovery |
| `Traditional` | Tracker/DHT only | Compatibility mode |

## Time Windows

SRTs are valid for a bounded time window. When a window expires:

1. Derive a new seed with updated `t0`
2. Compute new schedule
3. Continue discovery in the new window

```rust
// Check if refresh needed
if srt.remaining_secs() < 60 {
    srt = srt.refresh();  // New t0, new seed, new schedule
}
```

This provides:
- **Forward secrecy**: Old schedules don't reveal future ones
- **Bounded state**: No need to track all historical windows
- **NAT adaptation**: Fresh ports help with symmetric NAT

## Peer Exchange (PEX)

Once peers discover each other via SRT, they can exchange known peers:

```rust
#[derive(Serialize, Deserialize)]
pub struct PeerExchange {
    pub added: Vec<PexPeer>,    // New peers since last exchange
    pub dropped: Vec<[u8; 32]>, // Removed peer IDs
}
```

This bootstraps the swarm faster once initial discovery succeeds.

## Security Considerations

### Privacy

- **No central tracking**: No tracker sees peer lists
- **No DHT pollution**: No public DHT announces
- **Bounded probing**: Only active during time windows

### Abuse Prevention

- **Rate limiting**: Slot-based probing naturally limits probe rate
- **Identity constraints**: Optional peer ID whitelisting
- **Window expiry**: Stale SRTs become invalid

### Limitations

- **Clock synchronization**: Peers need roughly synchronized clocks (±slot_ms)
- **NAT traversal**: Still requires UDP hole punching
- **Initial discovery**: First peer pair must both be probing

## API Reference

### Core Types

```rust
// Infohash (SHA1 or SHA256)
pub enum InfoHash {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

// Parsed .torrent file
pub struct TorrentMeta {
    pub info_hash: InfoHash,
    pub name: String,
    pub piece_length: u64,
    pub pieces: Vec<PieceHash>,
    pub files: Vec<FileInfo>,
    pub srt_extension: Option<SrtExtension>,
    // ...
}

// SRT for a torrent swarm
pub struct SwarmSrt {
    pub token: SemanticRendezvousToken,
    pub info_hash: InfoHash,
    pub creation_time: u64,
}

// Peer discovery state
pub struct SwarmDiscovery {
    srt: SwarmSrt,
    local_peer: PeerId,
    config: DiscoveryConfig,
    peers: HashSet<[u8; 32]>,
}
```

### Key Functions

```rust
// Derive SRT from infohash
let srt = SwarmSrt::from_infohash(info_hash);

// Parse magnet with SRT
let magnet = MagnetUri::parse("magnet:?...")?;
if magnet.has_srt() {
    let srt = SwarmSrt::from_uri(&magnet.srt_uri.unwrap(), *magnet.primary_hash())?;
}

// Create enhanced magnet
let uri = magnet.with_srt(srt.to_uri()?).to_uri();

// Parse .torrent
let meta = parse_torrent(&data)?;
if meta.has_srt() {
    let srt = SwarmSrt::from_torrent_meta(&meta)?;
}
```

## Example: Complete Flow

```rust
use rift_torrent::{
    MagnetUri, SwarmSrt, SwarmDiscovery, SwarmPeer,
    DiscoveryConfig, DiscoveryMode, InfoHash,
};
use rift_rndzv::PeerId;

// 1. Parse or create infohash
let info_hash = InfoHash::from_hex("0123456789abcdef0123456789abcdef01234567")?;

// 2. Derive SRT (deterministic from infohash)
let srt = SwarmSrt::from_infohash(info_hash);
println!("SRT URI: {}", srt.to_uri()?);

// 3. Create discovery instance
let local_peer = PeerId([0u8; 32]); // Your peer ID
let config = DiscoveryConfig {
    mode: DiscoveryMode::SrtThenDht,
    srt_timeout_ms: 30_000,
    max_peers: 50,
    ..Default::default()
};
let mut discovery = SwarmDiscovery::new(srt, local_peer, config);

// 4. Share via magnet URI
let magnet = MagnetUri::from_infohash(info_hash)
    .with_name("My Torrent")
    .with_srt(discovery.srt_uri()?);
println!("Share this: {}", magnet.to_uri());

// 5. Peer discovery loop (conceptual)
loop {
    // Check if SRT needs refresh
    if discovery.needs_refresh() {
        discovery.refresh_srt();
    }

    // Prune stale peers
    let dropped = discovery.prune_stale();

    // In practice, integrate with rift-rndzv for actual probing
    // discovery.discover_via_srt().await?
}
```

## Comparison with Traditional Methods

| Aspect | Tracker | DHT | SRT (rift-torrent) |
|--------|---------|-----|-------------------|
| Infrastructure | Central server | Bootstrap nodes | None |
| Privacy | Tracker sees all | Public announces | Probes only |
| Latency | HTTP round-trip | DHT lookups | Direct probing |
| Reliability | Single point | Distributed | Peer-to-peer |
| Offline | Fails | Needs bootstrap | Works if any peer online |

## Future Directions

1. **Hybrid SRT+DHT**: Use DHT hints to improve NAT traversal
2. **Multi-tracker fallback**: Graceful degradation chain
3. **Swarm health metrics**: Track SRT discovery success rates
4. **Browser support**: WASM bindings for web torrents

## Related Documentation

- [Predictive Rendezvous](predictive-rendezvous.md): Core SRT architecture
- [SRT Tooling](srt-tooling.md): CLI tools for SRT generation
- [Hybrid Modes](hybrid-modes.md): Combining SRT with other discovery methods
