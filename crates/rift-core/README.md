# rift-core

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
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

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
