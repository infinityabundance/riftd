# Browser Client Prototype (WASM)

This is an early, text-only WebAssembly client that exercises the Rift protocol in the browser.
It uses a lightweight WebSocket relay for transport, while keeping protocol framing and
AES-GCM payload encryption compatible with the existing protocol types.

## Scope (Phase 45)
- Text-only chat over a WebSocket relay
- Invite-based session creation
- WASM bindings for encode/decode, invite helpers, and E2EE payload encryption
- Minimal static HTML/JS demo

## Build
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
