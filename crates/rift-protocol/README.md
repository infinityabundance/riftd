# rift-protocol

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-protocol"><img src="https://img.shields.io/crates/v/rift-protocol.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-protocol"><img src="https://docs.rs/rift-protocol/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  Versioned wire protocol framing and message types for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-protocol` defines the on-the-wire format for all rift messages:

- **Message Types** — Chat, voice, control, and relay messages
- **Framing** — Length-prefixed binary encoding
- **Versioning** — Protocol version negotiation
- **Serialization** — Efficient bincode encoding/decoding

## Usage

```rust
use rift_protocol::{RiftMessage, ChatMessage, VoiceFrame};
use rift_core::PeerId;

// Create a chat message
let chat = ChatMessage::new(
    peer_id,
    timestamp,
    "Hello, world!".to_string(),
);

// Encode for transmission
let bytes = chat.encode()?;

// Decode on receive
let msg = RiftMessage::decode(&bytes)?;
```

## Protocol Documentation

See [PROTOCOL.md](https://github.com/infinityabundance/riftd/blob/main/PROTOCOL.md) for the full protocol specification.

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-core](https://crates.io/crates/rift-core) | Core types and identity |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking layer |
| [rift-media](https://crates.io/crates/rift-media) | Audio encoding for voice frames |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
