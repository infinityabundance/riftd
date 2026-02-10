# rift-discovery

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-discovery"><img src="https://img.shields.io/crates/v/rift-discovery.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-discovery"><img src="https://docs.rs/rift-discovery/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  Peer discovery via mDNS and DHT for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-discovery` combines local and wide-area peer discovery:

- **mDNS Discovery** — Find peers on the local network automatically
- **DHT Discovery** — Find peers anywhere on the internet
- **Service Browsing** — Discover rift channels and peers
- **Peer Announcement** — Advertise your presence
- **Unified API** — Single interface for all discovery methods

## Usage

```rust
use rift_discovery::Discovery;

let discovery = Discovery::new(peer_id).await?;

// Start announcing
discovery.announce(channel_id).await?;

// Discover peers (combines mDNS + DHT)
let mut stream = discovery.discover();
while let Some(peer) = stream.next().await {
    println!("Found peer: {:?}", peer);
}
```

## How Discovery Works

1. **LAN** — mDNS broadcasts find peers on the same network instantly
2. **Internet** — DHT queries find peers anywhere (requires bootstrap)
3. **Invites** — Direct connection via invite tokens (see rift-rndzv)

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-dht](https://crates.io/crates/rift-dht) | DHT implementation |
| [rift-rndzv](https://crates.io/crates/rift-rndzv) | Invite-based rendezvous |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking layer |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
