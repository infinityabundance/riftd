# rift-web-chat

WebAssembly browser chat client for Rift over WebSocket relay.

This crate provides a simple, self-contained API for text-only encrypted chat in the browser. It connects to the `rift-ws-relay` WebSocket server and uses the same protocol framing and AES-GCM encryption as the rest of the Rift stack.

## Features

- **WebSocket transport** via `web-sys` - no additional JS dependencies
- **AES-GCM encryption** compatible with existing Rift protocol
- **Invite-based sessions** - create/join with `rift://z/...` invite URLs
- **Callback-based API** for easy JavaScript integration
- **Minimal footprint** - compiles to ~50KB gzipped WASM

## Installation

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
cd crates/rift-web-chat
wasm-pack build --target web
```

## Usage

```javascript
import init, { WebChat, create_invite, inspect_invite } from './pkg/rift_web_chat.js';

await init();

// Create an invite for a new room
const invite = create_invite("my-room", null);
console.log("Invite:", invite);

// Or inspect an existing invite
const info = inspect_invite(invite);
console.log("Channel:", info.channel_name);

// Connect to the relay
const chat = new WebChat("ws://localhost:8787/ws", invite);

// Set up event handlers
chat.on_message((msg) => {
    console.log(`[${msg.from.slice(0, 8)}]: ${msg.text}`);
});

chat.on_peer_event((event) => {
    console.log(`Peer ${event.peer_id.slice(0, 8)} ${event.event}`);
});

chat.on_connect(() => {
    console.log("Connected! My peer ID:", chat.peer_id);
    chat.send("Hello, world!");
});

chat.on_error((err) => {
    console.error("Error:", err.error);
});

// Later: disconnect
chat.disconnect();
```

## API

### `WebChat`

Main chat client class.

| Method | Description |
|--------|-------------|
| `new WebChat(relay_url, invite_url)` | Connect to relay and join room |
| `send(text)` | Send a text message |
| `on_message(callback)` | Set handler for incoming messages |
| `on_peer_event(callback)` | Set handler for join/leave events |
| `on_connect(callback)` | Set handler for connection established |
| `on_disconnect(callback)` | Set handler for disconnection |
| `on_error(callback)` | Set handler for errors |
| `disconnect()` | Close the connection |
| `peer_id` | Your peer ID (hex string) |
| `room` | Channel/room name |
| `session_id` | Session ID (hex string) |
| `is_connected` | Connection state |

### Standalone Functions

| Function | Description |
|----------|-------------|
| `create_invite(channel, password?)` | Generate a new invite URL |
| `inspect_invite(url)` | Parse invite without joining |
| `generate_peer_id()` | Generate a random peer ID |

## Running the Relay

```bash
cargo run -p rift-ws-relay
```

The relay listens on `ws://localhost:8787/ws` by default.

## Protocol

Messages are encrypted with AES-256-GCM using the invite's channel key, then base64-encoded and wrapped in a JSON envelope for the WebSocket relay:

```json
{"type": "data", "room": "<session_id>", "peer_id": "<peer_id>", "data": "<base64-frame>"}
```

The inner frame uses the standard Rift protocol format (`RFT1` magic, version, length, bincode-encoded header + encrypted payload).

## License

Apache-2.0 OR MIT
