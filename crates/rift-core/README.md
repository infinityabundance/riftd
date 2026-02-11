# rift-core

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-core"><img src="https://img.shields.io/crates/v/rift-core.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-core"><img src="https://docs.rs/rift-core/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
  <a href="https://doi.org/10.5281/zenodo.18528430"><img src="https://zenodo.org/badge/DOI/10.5281/zenodo.18528430.svg" alt="DOI"></a>
</p>

<p align="center">
  Core types, identity, cryptography, and invites for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-core` provides the foundational building blocks used by all other rift crates:

- **Identity** — Ed25519 keypair generation and management
- **PeerId** — Unique peer identifiers derived from public keys
- **Key Exchange** — X25519 for establishing shared secrets
- **Noise Protocol** — Session establishment helpers
- **Invites** — Token generation and parsing for peer discovery
- **Key Derivation** — HKDF-based key derivation utilities

## Usage

```rust
use rift_core::{Identity, PeerId};

// Generate a new identity
let identity = Identity::generate();
let peer_id = identity.peer_id();

// Or load from disk
let identity = Identity::load_or_generate()?;
```

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-protocol](https://crates.io/crates/rift-protocol) | Wire protocol and message types |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking and E2EE |
| [rift-sdk](https://crates.io/crates/rift-sdk) | High-level SDK |

## Citation

If you use riftd in academic work, please cite:

> de Beer, R. (2026). *Predictive Rendezvous: Time–Intent–Deterministic Peer Coordination Without Infrastructure*. Zenodo. https://doi.org/10.5281/zenodo.18528430

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
