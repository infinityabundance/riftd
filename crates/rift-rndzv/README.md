# rift-rndzv

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-rndzv"><img src="https://img.shields.io/crates/v/rift-rndzv.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-rndzv"><img src="https://docs.rs/rift-rndzv/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  Rendezvous protocol for NAT traversal in the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-rndzv` implements Predictive Rendezvous — a novel approach to NAT traversal:

- **SRT Tokens** — Short Rendezvous Tokens for easy sharing (QR codes, links)
- **Predictive Scheduling** — Time-coordinated hole punching
- **NAT Coordination** — Synchronized connection attempts
- **Invite Flow** — Generate and accept peer invitations
- **Fallback Handling** — Graceful degradation to relay

## Usage

```rust
use rift_rndzv::{RndzvConnector, SrtToken};

let connector = RndzvConnector::new();

// Generate an invite
let srt = connector.generate_srt(space_id, &identity)?;
println!("Share this: {}", srt.to_uri());

// Connect via invite
let peer = connector.connect_srt(&srt_uri).await?;
```

## SRT Format

SRTs are compact, shareable tokens:
```
rift://srt/<base64-encoded-token>
```

See [docs/srt-invites.md](https://github.com/infinityabundance/riftd/blob/main/docs/srt-invites.md) for UX patterns.

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-nat](https://crates.io/crates/rift-nat) | STUN/TURN primitives |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking (uses rndzv) |
| [rift-sdk](https://crates.io/crates/rift-sdk) | High-level invite API |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
