# rift-mesh

Mesh networking, session management, and E2EE for Rift P2P.

## Features

- Noise protocol encrypted sessions
- Peer discovery and connection management
- NAT traversal with STUN/TURN fallback
- End-to-end encrypted messaging
- Voice and text chat support

## Usage

```rust
use rift_mesh::MeshNode;

let node = MeshNode::new(config).await?;
node.connect(peer_addr).await?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
