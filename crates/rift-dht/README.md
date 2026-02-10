# rift-dht

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-dht"><img src="https://img.shields.io/crates/v/rift-dht.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-dht"><img src="https://docs.rs/rift-dht/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  Distributed hash table for peer discovery in the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-dht` enables wide-area peer discovery without central servers:

- **Kademlia DHT** — Built on libp2p's battle-tested implementation
- **Peer Announcement** — Publish your presence to the network
- **Peer Lookup** — Find peers by channel or peer ID
- **Bootstrap Nodes** — Connect to the network via known entry points
- **Automatic Routing** — Efficient peer routing tables

## Usage

```rust
use rift_dht::DhtNode;

let dht = DhtNode::new(config).await?;

// Announce presence
dht.announce(peer_id, channel_id).await?;

// Find peers in a channel
let peers = dht.find_peers(channel_id).await?;
```

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-discovery](https://crates.io/crates/rift-discovery) | Combined mDNS + DHT discovery |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking layer |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
