# rift-nat

NAT traversal, STUN, and TURN support for Rift P2P.

## Features

- STUN client for reflexive address discovery
- TURN client for relay fallback
- NAT type detection
- ICE-lite candidate gathering

## Usage

```rust
use rift_nat::{StunClient, TurnClient};

let stun = StunClient::new("stun.l.google.com:19302");
let reflexive_addr = stun.get_reflexive_address().await?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
