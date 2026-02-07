# riftd
Serverless P2P voice + text chat over UDP with a mesh, NAT traversal, and optional relays.

`riftd` is a small, pragmatic alternative to heavyweight WebRTC stacks. It is designed to work on LANs and across the internet without central servers, using mDNS discovery, UDP hole punching, and peer relays when needed. The project includes a terminal UI (TUI) client and a protocol crate that can be reused by other applications.

## Highlights
- Pure P2P mesh: every peer talks to every peer.
- LAN discovery via mDNS.
- Internet invites + UDP hole punching (no STUN/TURN yet).
- Peer relay fallback when direct P2P fails.
- Opus voice with configurable quality.
- Versioned on-the-wire protocol in `crates/rift-protocol`.
- TUI client with chat, peer list, and voice controls.

## Current State
The repo contains a working voice + text mesh with:
- LAN discovery and invites.
- NAT traversal (multi-port UDP hole punching).
- Peer relay fallback + auto-upgrade to direct.
- TUI client with basic call/session semantics.

## Quick Start
1. Build:
```bash
cargo build -p rift
```

2. Generate identity once:
```bash
cargo run -p rift -- init-identity
```

3. LAN create + join (two terminals):
```bash
# Terminal A
cargo run -p rift -- create --channel gaming --voice --port 7777

# Terminal B
cargo run -p rift -- create --channel gaming --voice --port 7778
```

4. Internet (invite mode):
```bash
# Terminal A
cargo run -p rift -- create --channel gaming --voice --internet --port 7777
cargo run -p rift -- invite --channel gaming

# Terminal B (use invite string)
cargo run -p rift -- join --invite "rift://join/..."
```

## TUI Usage
- Type in the input box and press Enter to chat.
- `/call <peer_id>` to initiate a call.
- `/hangup` or `/bye` to end an active call.
- Incoming call: `a` to accept, `d` to decline.
- `m` toggles mute (disables mic capture).
- `Ctrl+A` toggles audio quality (low/medium/high).
- `Ctrl+Q` quits.
- `TAB` toggles focus between input and peer list.

The status bar shows:
- Channel name + peer count.
- Mic/PTT state.
- Quality preset.
- Call state.
- RX/TX activity dots.

## Config
Config file (optional):
`~/.config/rift/config.toml`

Example:
```toml
[user]
name = "alice"

[audio]
input_device = "default"
output_device = "default"
quality = "medium"    # low | medium | high
ptt = false
ptt_key = "f1"        # f1..f12 | space | ctrl_space | alt_space | ctrl_backtick | ctrl_semicolon
vad = true
mute_output = false

[network]
prefer_p2p = true
relay = false
local_ports = [7777, 7778, 7779]

[ui]
theme = "dark"
```

## Crates
- `rift-core`: identity, keys, invites, core types.
- `rift-protocol`: versioned framing + on-the-wire types.
- `rift-discovery`: LAN mDNS discovery.
- `rift-nat`: UDP hole punching.
- `rift-mesh`: mesh routing, relay, call/session handling.
- `rift-media`: audio capture/playback and Opus codec.
- `bin/rift`: TUI client.
- `rift-sdk`: high-level SDK for embedding Rift (Rust + C FFI).

## Docs
- `CODE.md`: high-level code map.
- `PROTOCOL.md`: protocol framing and message types.
- `ROADMAP.md`: planned next steps.

## Development
Tests:
```bash
cargo test -p rift
```

Note: The project uses UDP and raw terminal input; run in a real terminal emulator.
