# rift-dht

Distributed hash table for Rift P2P peer discovery.

## Features

- Kademlia DHT implementation via libp2p
- Peer announcement and discovery
- Bootstrap node support
- Automatic peer routing

## Usage

```rust
use rift_dht::DhtNode;

let dht = DhtNode::new(config).await?;
dht.announce(peer_id).await?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
