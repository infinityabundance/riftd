# rift-rndzv

Rendezvous protocol for Rift P2P NAT traversal.

## Features

- Secure rendezvous token exchange
- NAT hole-punching coordination
- Predictive connection establishment
- SRT (Short Rendezvous Token) for easy sharing

## Usage

```rust
use rift_rndzv::{RndzvConnector, RndzvConfig};

let connector = RndzvConnector::new();
// Use with rift-mesh for NAT traversal
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
