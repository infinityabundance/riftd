# rift-sdk

High-level SDK for building Rift P2P applications.

## Features

- Simple API for P2P voice and text chat
- Automatic peer discovery (mDNS, DHT)
- NAT traversal built-in
- E2EE by default
- Cross-platform (desktop, mobile, WASM)

## Usage

```rust
use rift_sdk::{RiftConfig, RiftClient};

let config = RiftConfig::default();
let client = RiftClient::new(config).await?;
client.join_room("my-room").await?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
