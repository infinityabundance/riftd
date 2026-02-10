# rift-core

Core types, identity, cryptography, and invites for the Rift P2P protocol.

## Features

- Ed25519 identity key generation and management
- X25519 key exchange for E2EE
- Noise protocol session establishment
- Peer ID and invite token handling
- HKDF key derivation

## Usage

```rust
use rift_core::{Identity, PeerId};

let identity = Identity::generate();
let peer_id = identity.peer_id();
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
