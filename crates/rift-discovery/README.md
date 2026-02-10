# rift-discovery

Peer discovery via mDNS and DHT for Rift P2P.

## Features

- Local network discovery via mDNS
- Wide-area discovery via DHT
- Automatic peer announcement
- Service browsing

## Usage

```rust
use rift_discovery::Discovery;

let discovery = Discovery::new(peer_id).await?;
let peers = discovery.discover().await;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
