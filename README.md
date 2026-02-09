# riftd

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="assets/riftd.svg" alt="[riftd](https://github.com/infinityabundance/riftd)" width="120">
  </a>
</p>

Serverless P2P voice + text chat over UDP with a mesh, NAT traversal, and optional relays.

`riftd` is a small, pragmatic alternative to heavyweight WebRTC stacks. It is designed to work on LANs and across the internet without central servers, using mDNS discovery, UDP hole punching, and peer relays when needed. The project includes a terminal UI (TUI) client and a protocol crate that can be reused by other applications.

## Highlights
- Pure P2P mesh: every peer talks to every peer (with relay fallback).
- LAN discovery via mDNS + internet discovery via invites and DHT.
- NAT traversal: UDP hole punching + STUN candidates + optional TURN fallback.
- End-to-end encryption for chat and voice (pairwise).
- Opus voice with configurable quality + QoS adaptation.
- Versioned on-the-wire protocol in `crates/rift-protocol`.
- TUI client with chat, peer list, and voice controls.

## Current State
The repo contains a working voice + text mesh with:
- LAN discovery, invites, and DHT discovery.
- NAT traversal (hole punching + STUN + optional TURN relay).
- Peer relay fallback + auto-upgrade to direct.
- Pairwise E2EE for chat + voice.
- TUI client with call/session semantics.

## Browser Prototype
There is an early WebAssembly browser spike for text-only chat over a WebSocket relay.
See `README.browser.md` for build and run instructions.

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
XDG_CONFIG_HOME=/tmp/rift2 cargo run -p rift -- create --channel gaming --voice --port 7778
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
- `rift-rndzv`: Predictive Rendezvous networking layer (SRTs, scheduling, runner).
- `rift-mesh`: mesh routing, relay, call/session handling.
- `rift-media`: audio capture/playback and Opus codec.
- `bin/rift`: TUI client.
- `rift-sdk`: high-level SDK for embedding Rift (Rust + C FFI).

## Docs
- `CODE.md`: high-level code map.
- `PROTOCOL.md`: protocol framing and message types.
- `docs/srt-tooling.md`: SRT generation and inspection tooling.
- `docs/srt-invites.md`: SRT invite UX and sharing patterns.
- `docs/predictive-rendezvous.md`: Predictive Rendezvous architecture and SRT format.
- `docs/future-directions.md`: cross-domain synthesis and future directions.
- `docs/hybrid-coordination.md`: optional hybrid coordination patterns.
- `docs/hybrid-modes.md`: optional hybrid modes (rndzv + relay / DHT hints).
- `docs/rndzv-1x-contract.md`: stable rndzv 1.x public contract.
- `docs/rndzv-2.0-plan.md`: phased plan for rndzv 2.0.
- `docs/pr-security.md`: security and abuse considerations for Predictive Rendezvous.
- `docs/formalization.md`: minimal formal model for Predictive Rendezvous.
- `docs/cross-protocol.md`: protocol-agnostic mapping across domains.
- `docs/design-rationale.md`: why PR exists and what it does not solve.
- `docs/index.md`: documentation index and phase status.
- `SECURITY.md`: threat model and security checklist.
- `TURN_GUIDE.md`: self-hosted TURN setup and config.
- `CHANGELOG.md`: release history.
- `RELEASE_CHECKLIST.md`: release steps.
- `OPTIMIZATION_REPORT.md`: performance notes.
- `PHASE34_PLAN.md`: implementation plan for ICE/E2EE reliability work.
- `ROADMAP.md`: planned next steps.

## rndzv CLI tools
Basic SRT utilities:
```bash
rift rndzv generate
rift rndzv inspect <srt-uri>
```

Manual rendezvous demo:
```bash
# Terminal A
rift rndzv listen <space-id-hex> --repl

# Terminal B (paste the printed SRT URI)
rift rndzv connect <srt-uri> --repl
```

## Development
Tests:
```bash
cargo test -p rift
```

Note: The project uses UDP and raw terminal input; run in a real terminal emulator.
