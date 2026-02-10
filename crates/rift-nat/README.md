# rift-nat

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-nat"><img src="https://img.shields.io/crates/v/rift-nat.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-nat"><img src="https://docs.rs/rift-nat/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  NAT traversal, STUN, and TURN support for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-nat` handles the hard problem of connecting peers behind NATs:

- **STUN Client** — Discover your public IP and port (server-reflexive address)
- **TURN Client** — Relay fallback when direct connection fails
- **NAT Detection** — Identify NAT type (full cone, restricted, symmetric)
- **Hole Punching** — Coordinated UDP hole punch attempts
- **ICE-lite** — Simplified ICE candidate gathering

## Usage

```rust
use rift_nat::{StunClient, NatProbe};

// Discover reflexive address via STUN
let stun = StunClient::new("stun.l.google.com:19302").await?;
let reflexive = stun.binding_request().await?;
println!("Public address: {}", reflexive);

// Probe NAT type
let nat_type = NatProbe::detect().await?;
```

## TURN Setup

For self-hosted TURN relay, see [TURN_GUIDE.md](https://github.com/infinityabundance/riftd/blob/main/TURN_GUIDE.md).

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-rndzv](https://crates.io/crates/rift-rndzv) | Rendezvous coordination |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking (uses NAT traversal) |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
