# rift-mesh

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-mesh"><img src="https://img.shields.io/crates/v/rift-mesh.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-mesh"><img src="https://docs.rs/rift-mesh/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
  <a href="https://doi.org/10.5281/zenodo.18528430"><img src="https://zenodo.org/badge/DOI/10.5281/zenodo.18528430.svg" alt="DOI"></a>
</p>

<p align="center">
  Mesh networking, session management, and E2EE for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-mesh` is the heart of riftd's networking:

- **Mesh Topology** — Every peer connects to every peer
- **Noise Sessions** — Encrypted channels using Noise_XX
- **E2EE** — End-to-end encryption for all messages
- **NAT Traversal** — Automatic hole punching with fallback
- **Peer Relay** — Route through peers when direct fails
- **Session Management** — Handle joins, leaves, reconnects

## Usage

```rust
use rift_mesh::{MeshNode, MeshConfig};

let config = MeshConfig::default();
let node = MeshNode::new(config).await?;

// Join a channel
node.join(channel_id).await?;

// Send a message
node.send_chat(peer_id, "Hello!").await?;

// Handle incoming messages
while let Some(event) = node.next_event().await {
    match event {
        MeshEvent::Chat { from, message } => { /* ... */ }
        MeshEvent::Voice { from, frame } => { /* ... */ }
        MeshEvent::PeerJoined(peer) => { /* ... */ }
    }
}
```

## Features

- `predictive-rendezvous` (default) — Enable SRT-based NAT traversal

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-core](https://crates.io/crates/rift-core) | Identity and crypto primitives |
| [rift-protocol](https://crates.io/crates/rift-protocol) | Message types |
| [rift-nat](https://crates.io/crates/rift-nat) | STUN/TURN support |
| [rift-rndzv](https://crates.io/crates/rift-rndzv) | Predictive Rendezvous |
| [rift-sdk](https://crates.io/crates/rift-sdk) | High-level API |

## Citation

If you use riftd in academic work, please cite:

> de Beer, R. (2026). *Predictive Rendezvous: Time–Intent–Deterministic Peer Coordination Without Infrastructure*. Zenodo. https://doi.org/10.5281/zenodo.18528430

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
