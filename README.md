# riftd

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="assets/riftd.svg" alt="[riftd](https://github.com/infinityabundance/riftd)" width="120">
  </a>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="Rust"></a>
  <a href="https://crates.io/crates/rift-sdk"><img src="https://img.shields.io/crates/v/rift-sdk.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-sdk"><img src="https://docs.rs/rift-sdk/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
  <a href="https://doi.org/10.5281/zenodo.18528430"><img src="https://zenodo.org/badge/DOI/10.5281/zenodo.18528430.svg" alt="DOI"></a>
</p>
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/infinityabundance/riftd)

Serverless P2P voice + text chat over UDP with a mesh, NAT traversal, and optional relays.

`riftd` is a small, pragmatic alternative to heavyweight WebRTC stacks. It is designed to work on LANs and across the internet without central servers, using mDNS discovery, UDP hole punching, and peer relays when needed. The project includes a terminal UI (TUI) client and a protocol crate that can be reused by other applications.

### What makes riftd more than just "WebRTC, but in Rust":

> **Predictive Rendezvous: Time–Intent–Deterministic Peer Coordination Without Infrastructure**  
> Riaan de Beer, 2026
> **[https://doi.org/10.5281/zenodo.18528430]**

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

## Native Clients

### Qt Desktop (Windows / macOS / Linux)
A unified Qt6/QML desktop client with:
- Dark mode UI
- System tray integration
- Voice calls with audio device selection
- Channel management and chat

Build instructions:
```bash
# Build SDK with FFI exports
cargo build -p rift-sdk --release --features ffi

# Linux (requires Qt6, opus, alsa)
cmake -S clients/rift-qt-unified -B build-qt
cmake --build build-qt --config Release

# macOS (requires Qt6 via Homebrew)
cmake -S clients/rift-qt-unified -B build-qt \
  -DCMAKE_PREFIX_PATH=$(brew --prefix qt@6)
cmake --build build-qt --config Release

# Windows (requires Qt6 + MSVC)
cmake -S clients/rift-qt-unified -B build-qt
cmake --build build-qt --config Release
```

### Android
A Kotlin/Jetpack Compose app with:
- JNI bindings to the Rift SDK
- Voice calls with foreground service
- Audio focus and Bluetooth routing

Build instructions:
```bash
# Build SDK for Android targets
rustup target add aarch64-linux-android armv7-linux-androideabi
cargo install cargo-ndk
cargo ndk -t arm64-v8a -t armeabi-v7a -o android/rift-sdk-android/src/main/jniLibs \
  build -p rift-sdk --release --features android

# Build APK
cd android
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ./gradlew assembleDebug
```

## Browser Prototype
There is an early WebAssembly browser spike for text-only chat over a WebSocket relay.
See `docs/README.browser.md` for build and run instructions.

## SRT-Based Torrents
The `rift-torrent` crate enables zero-infrastructure torrent peer discovery using Predictive Rendezvous. Instead of trackers or DHT bootstrap nodes, peers derive identical rendezvous schedules from the infohash alone.

```rust
use rift_torrent::{MagnetUri, SwarmSrt, InfoHash};

// Parse a magnet URI
let magnet = MagnetUri::parse("magnet:?xt=urn:btih:...")?;

// Derive SRT from infohash (no tracker needed!)
let srt = SwarmSrt::from_infohash(*magnet.primary_hash());

// Share the SRT-enhanced magnet
let enhanced = magnet.with_srt(srt.to_uri()?);
println!("{}", enhanced.to_uri());
// magnet:?xt=urn:btih:...&xs=riftd-srt://v1?space=...&seed=...
```

See `docs/rift-torrent.md` for detailed documentation on how it works.

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
# Terminal A (internet is enabled by default)
cargo run -p rift -- create --channel gaming --voice --port 7777
cargo run -p rift -- invite --channel gaming

# Terminal B (use invite string)
cargo run -p rift -- join --invite "rift://join/..."
```

Note: Internet connectivity (STUN, DHT, relays) is enabled by default. Use `--no-internet` for LAN-only mode.

## TUI Usage
- Type in the input box and press Enter to chat.
- `/call <peer_id>` to initiate a call.
- `/hangup` or `/bye` to end an active call.
- Incoming call: `a` to accept, `d` to decline.
- `m` toggles mute (disables mic capture).
- `Ctrl+G` shows invite command in chat (easy copy for sharing).
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
- [`rift-core`](https://crates.io/crates/rift-core): identity, keys, invites, core types.
- [`rift-protocol`](https://crates.io/crates/rift-protocol): versioned framing + on-the-wire types.
- [`rift-discovery`](https://crates.io/crates/rift-discovery): LAN mDNS discovery.
- [`rift-dht`](https://crates.io/crates/rift-dht): distributed hash table for peer discovery.
- [`rift-nat`](https://crates.io/crates/rift-nat): UDP hole punching.
- [`rift-rndzv`](https://crates.io/crates/rift-rndzv): Predictive Rendezvous networking layer (SRTs, scheduling, runner).
- [`rift-mesh`](https://crates.io/crates/rift-mesh): mesh routing, relay, call/session handling.
- [`rift-media`](https://crates.io/crates/rift-media): audio capture/playback and Opus codec.
- [`rift-metrics`](https://crates.io/crates/rift-metrics): metrics and telemetry.
- [`rift-sdk`](https://crates.io/crates/rift-sdk): high-level SDK for embedding Rift (Rust + C FFI + JNI).
- [`rift-wasm`](https://crates.io/crates/rift-wasm): WebAssembly bindings for browser clients.
- [`rift-web-chat`](https://crates.io/crates/rift-web-chat): WebSocket-based browser chat client (WASM).
- [`rift-torrent`](https://crates.io/crates/rift-torrent): SRT-based torrent peer discovery (zero-infrastructure).
- `bin/rift`: TUI client.
- `clients/rift-qt-unified`: Qt6/QML desktop client (Windows / macOS / Linux).
- `android/`: Kotlin/Compose Android app with JNI bindings.

## Docs
All documentation is in the `docs/` folder:
- `docs/CODE.md`: high-level code map.
- `docs/PROTOCOL.md`: protocol framing and message types.
- `docs/rift-torrent.md`: SRT-based torrent peer discovery.
- `docs/srt-tooling.md`: SRT generation and inspection tooling.
- `docs/srt-invites.md`: SRT invite UX and sharing patterns.
- `docs/predictive-rendezvous-overview.md`: conceptual overview and theory of Predictive Rendezvous.
- `docs/predictive-rendezvous.md`: Predictive Rendezvous implementation guide and SRT format.
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
- `docs/SECURITY.md`: threat model and security checklist.
- `docs/TURN_GUIDE.md`: self-hosted TURN setup and config.
- `docs/CHANGELOG.md`: release history.
- `docs/RELEASE_CHECKLIST.md`: release steps.
- `docs/OPTIMIZATION_REPORT.md`: performance notes.
- `docs/PHASE34_PLAN.md`: implementation plan for ICE/E2EE reliability work.
- `docs/ROADMAP.md`: planned next steps.

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
