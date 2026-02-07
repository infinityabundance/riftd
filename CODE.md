# Code Map
This document describes how the repository is organized and how the major pieces fit together.

## Workspace Layout
- `bin/rift`: TUI client and CLI entrypoint.
- `crates/rift-core`: identity, keys, invite encoding/decoding, core IDs.
- `crates/rift-protocol`: on-the-wire framing, versioning, and protocol types.
- `crates/rift-discovery`: LAN mDNS discovery.
- `crates/rift-nat`: UDP hole punching for NAT traversal.
- `crates/rift-mesh`: mesh routing, relays, call/session handling.
- `crates/rift-media`: audio capture/playback, Opus encode/decode, mixing.

## Key Concepts
- **PeerId**: stable identity for a peer. Generated once and stored locally.
- **Channel**: shared swarm name for discovery and default group session.
- **Session**: explicit call/session semantics on top of the mesh.
- **Rift Frame**: versioned binary frame that carries Control/Text/Voice payloads.

## Crate Details

### `rift-core`
Primary responsibilities:
- Identity creation/loading (`Identity`).
- Key storage.
- Invite encoding/decoding (`Invite`).
- Core identifiers: `PeerId`, `ChannelId`, `MessageId`.

Where to start:
- `crates/rift-core/src/identity.rs`
- `crates/rift-core/src/invite.rs`
- `crates/rift-core/src/message.rs`

### `rift-protocol`
Defines the canonical on-the-wire protocol:
- `RiftFrameHeader` + `RiftPayload`.
- Stream multiplexing: Control, Text, Voice, Custom.
- Protocol versioning and selection.
- Call/session types.

Where to start:
- `crates/rift-protocol/src/lib.rs`

### `rift-discovery`
LAN discovery using mDNS:
- Periodic advertisement + browse windows.
- Returns peers with `PeerId` and socket address.

Where to start:
- `crates/rift-discovery/src/lib.rs`

### `rift-nat`
UDP hole punching:
- Binds multiple local ports.
- Attempts to reach a peer’s external address list.
- Returns first bidirectional socket that succeeds.

Where to start:
- `crates/rift-nat/src/lib.rs`

### `rift-mesh`
Mesh networking and routing:
- Noise-encrypted UDP sessions.
- Peer list exchange.
- Relay routing when direct P2P fails.
- Auto-upgrade to direct.
- Session manager for calls.

Where to start:
- `crates/rift-mesh/src/lib.rs`

### `rift-media`
Audio pipeline:
- Audio capture + playback (`cpal`).
- Opus encoder/decoder.
- Audio mixer with simple jitter buffering.

Where to start:
- `crates/rift-media/src/lib.rs`

### `bin/rift`
Terminal UI:
- Input handling, chat, peer list, status bar.
- PTT controls and VAD.
- Call initiation/accept/decline.

Where to start:
- `bin/rift/src/main.rs`
- `bin/rift/src/config.rs`

## Data Flow Overview
1. **Discovery**
   - LAN: mDNS yields peer addresses.
   - Internet: invites provide bootstrap peers.
2. **Handshake**
   - Noise handshake over UDP.
3. **Mesh**
   - Control/Text/Voice frames sent over encrypted UDP.
4. **Voice**
   - Mic capture → Opus encode → mesh broadcast.
   - Receive → Opus decode → mixer → output.
5. **Relays**
   - If direct P2P fails, use a relay-capable peer.
6. **Sessions**
   - Control messages manage call state.
   - Voice packets tagged with `SessionId`.

## Configuration
Config is optional and loaded from:
`~/.config/rift/config.toml`

See `README.md` for a full example.
