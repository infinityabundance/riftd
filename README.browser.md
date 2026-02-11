# Browser Client Prototype (WASM)

This is an early, text-only WebAssembly client that exercises the Rift protocol in the browser.
It uses a lightweight WebSocket relay for transport, while keeping protocol framing and
AES-GCM payload encryption compatible with the existing protocol types.

## Crates

There are two WASM crates for browser integration:

| Crate | Description | Use Case |
|-------|-------------|----------|
| `rift-wasm` | Low-level protocol bindings | Custom WebSocket/WebRTC transport |
| `rift-web-chat` | High-level chat client | Quick browser chat integration |

**`rift-web-chat`** is recommended for most browser chat applications. It includes WebSocket transport, connection management, and a callback-based API.

**`rift-wasm`** provides lower-level primitives (frame encode/decode, encryption, invite handling) for building custom transport layers.

## Quick Start with rift-web-chat

```bash
# Build the WASM package
wasm-pack build crates/rift-web-chat --target web

# Start the relay
cargo run -p rift-ws-relay
```

```javascript
import init, { WebChat, create_invite } from './pkg/rift_web_chat.js';

await init();
const invite = create_invite("my-room", null);
const chat = new WebChat("ws://localhost:8787/ws", invite);

chat.on_message((msg) => console.log(`${msg.from}: ${msg.text}`));
chat.on_connect(() => chat.send("Hello!"));
```

See [`crates/rift-web-chat/README.md`](crates/rift-web-chat/README.md) for full API documentation.

## Low-Level Usage with rift-wasm

### Prereqs
- `wasm-pack`
- Optional: `trunk` (for live reload)

### Build the WASM package
```bash
wasm-pack build crates/rift-wasm --target web --out-dir www/pkg
```

### Run the relay
```bash
cargo run -p rift-ws-relay
```

### Serve the demo
```bash
python3 -m http.server --directory www 8080
```
Open `http://localhost:8080` in two tabs and paste the same invite link.

### Trunk (optional)
```bash
trunk serve www/index.html --dist www/dist
```

## Notes
- The relay is intentionally minimal and only handles small text frames.
- Encryption uses the invite channel key for now (pairwise E2EE handshake will be added next).
- The WASM client is not yet connected to UDP mesh, TURN, or audio.
