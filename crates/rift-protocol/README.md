# rift-protocol

Versioned wire protocol framing and message types for Rift P2P.

## Features

- Binary message encoding/decoding
- Protocol versioning
- Chat, voice, and control message types
- Efficient serialization with bincode

## Usage

```rust
use rift_protocol::{RiftMessage, ChatMessage};

let msg = ChatMessage::new(peer_id, timestamp, "Hello!".into());
let encoded = msg.encode()?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
