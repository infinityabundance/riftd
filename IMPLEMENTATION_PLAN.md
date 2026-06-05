# riftd Implementation Plan: Gap Closure Roadmap
**Date:** 2026-02-15  
**Based on:** AUDIT_REPORT.md findings  
**Goal:** Systematic closure of implementation gaps and completion of stub/partial features

---

## Overview

This plan provides a phased approach to completing all stub implementations, closing documentation gaps, and bringing the codebase to full production readiness.

**Timeline:** 12-16 weeks  
**Phases:** 5 major phases  
**Estimated PRs:** 20-25 total

---

## Phase 0: Cleanup and Documentation (Week 1) 🧹

**Goal:** Remove confusion, clarify status, improve accuracy.

### Tasks

#### 0.1: Remove Misleading Comments
**File:** `crates/rift-rndzv/src/api.rs`

Remove or update "(stub)" comments on fully implemented functions:
- Line 358: `RndzvSession::open_channel()`
- Line 406: `RndzvChannel::send()`
- Line 486: `RndzvChannel::recv()`
- Line 703: `RndzvConnector::connect()`
- Line 1130: `RndzvListener::accept()`

**Alternative:** Replace with `// Fully implemented` or remove entirely.

**PR:** `chore: remove misleading stub comments from rift-rndzv`

#### 0.2: Update README.md for Accuracy
**File:** `README.md`

Change lines 30-31 from:
```markdown
- NAT traversal: UDP hole punching + STUN candidates + optional TURN fallback.
```

To:
```markdown
- NAT traversal: UDP hole punching (STUN candidates and ICE-lite in progress - see Phase 35).
```

**PR:** `docs: clarify NAT traversal implementation status in README`

#### 0.3: Enhance Browser Security Warning
**File:** `docs/README.browser.md`

Add prominent warning box at top:
```markdown
> **⚠️ SECURITY WARNING**
> The browser client currently uses a shared channel key for encryption instead of pairwise E2EE.
> This means the relay server could decrypt messages. Do not use for sensitive communications.
> Pairwise Noise handshake is planned for the next release.
```

**PR:** `docs: add security warning to browser client README`

#### 0.4: Create Feature Status Matrix
**File:** `docs/FEATURE_STATUS.md` (new)

Create a simple status matrix:
```markdown
| Feature | Status | Since | Tracking |
|---------|--------|-------|----------|
| P2P Mesh | ✅ Stable | v0.1 | - |
| mDNS Discovery | ✅ Stable | v0.1 | - |
| DHT Discovery | ✅ Stable | v0.1 | - |
| UDP Hole Punching | ✅ Stable | v0.1 | - |
| STUN Candidates | 🚧 In Progress | - | Phase 35.1 |
| ICE-lite | 🚧 Planned | - | Phase 35.2-35.3 |
| Browser E2EE | ⚠️ Partial | v0.1 | Issue #TBD |
| Browser P2P | ❌ Planned | - | - |
```

**PR:** `docs: add feature status matrix`

**Deliverables:**
- [ ] 4 PRs merged
- [ ] Documentation accuracy at 95%+
- [ ] No misleading comments in codebase

**Time:** 2-3 days

---

## Phase 1: Browser Security Fix (Week 2-3) 🔐

**Goal:** Fix insecure shared key in browser client.

### Background
Currently, `rift-wasm` and `rift-web-chat` use the invite channel key for all encryption. This is insecure because the relay server can decrypt messages.

### Tasks

#### 1.1: Add Noise Handshake to rift-wasm
**File:** `crates/rift-wasm/src/crypto.rs` (new or extend existing)

**Steps:**
1. Add `snow` dependency to `rift-wasm/Cargo.toml`
2. Implement Noise XX handshake pattern:
   - Initiator sends `-> e`
   - Responder sends `<- e, ee, s, es`
   - Initiator sends `-> s, se`
3. Derive transport keys from handshake
4. Export `start_handshake()`, `process_handshake()`, `finish_handshake()` to JS

**Dependencies:**
```toml
[dependencies]
snow = "0.9"
```

**WASM bindings:**
```rust
#[wasm_bindgen]
pub struct NoiseHandshake {
    state: snow::HandshakeState,
}

#[wasm_bindgen]
impl NoiseHandshake {
    pub fn new_initiator(static_key: &[u8]) -> Result<NoiseHandshake, JsValue>;
    pub fn new_responder(static_key: &[u8]) -> Result<NoiseHandshake, JsValue>;
    pub fn write_message(&mut self, payload: &[u8]) -> Result<Vec<u8>, JsValue>;
    pub fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>, JsValue>;
    pub fn into_transport_mode(self) -> Result<NoiseTransport, JsValue>;
}
```

**PR:** `feat(wasm): add Noise handshake for pairwise E2EE`

#### 1.2: Update rift-web-chat to Use Handshake
**File:** `crates/rift-web-chat/src/lib.rs`

**Changes:**
1. Add handshake state to `WebChat` struct
2. Perform handshake after WebSocket connect:
   - If initiator: send handshake init message
   - If responder: wait for handshake init, respond
3. Only switch to chat mode after handshake complete
4. Use transport keys for all subsequent encryption

**PR:** `feat(web-chat): use Noise handshake instead of shared key`

#### 1.3: Update Browser Client Example
**File:** `www/index.html` or demo files

Update to handle handshake messages before displaying chat UI.

**PR:** `feat(www): update browser demo for Noise handshake`

#### 1.4: Update Documentation
**Files:** 
- `docs/README.browser.md`
- `crates/rift-web-chat/README.md`

Remove security warning, add handshake documentation.

**PR:** `docs: update browser docs after E2EE fix`

**Deliverables:**
- [ ] Noise handshake implemented in WASM
- [ ] Browser client uses pairwise E2EE
- [ ] Security warning removed
- [ ] Tests for handshake roundtrip

**Time:** 1-2 weeks

---

## Phase 2: STUN Client (Phase 35.1) (Week 4-6) 🌐

**Goal:** Implement STUN client for public address discovery.

### Background
Currently, `rift-nat` has basic UDP hole punching but no STUN integration. Phase 35.1 requires STUN client to gather srflx (server-reflexive) candidates.

### Tasks

#### 2.1: Implement STUN Client
**File:** `crates/rift-nat/src/stun.rs` (new)

**Steps:**
1. Implement STUN RFC 5389 Binding Request/Response
2. Support multiple STUN servers with fallback
3. Return public IP + port as candidate
4. Add timeout and retry logic
5. Support both IPv4 and IPv6

**API:**
```rust
pub struct StunClient {
    servers: Vec<SocketAddr>,
    timeout: Duration,
}

impl StunClient {
    pub async fn discover_public_addr(&self, local_socket: &UdpSocket) 
        -> Result<Candidate, StunError>;
    
    pub async fn discover_all_candidates(&self, local_addrs: Vec<SocketAddr>) 
        -> Result<Vec<Candidate>, StunError>;
}

pub struct Candidate {
    pub typ: CandidateType,
    pub addr: SocketAddr,
    pub priority: u32,
}

pub enum CandidateType {
    Host,
    ServerReflexive,
    Relayed,
}
```

**Dependencies:**
```toml
[dependencies]
bytes = "1.0"
tokio = { version = "1.0", features = ["net", "time"] }
```

**PR:** `feat(nat): implement STUN client (Phase 35.1)`

#### 2.2: Add Candidate Gathering
**File:** `crates/rift-nat/src/lib.rs`

**Steps:**
1. Add `gather_candidates()` function:
   - List all local network interfaces (host candidates)
   - Query STUN servers for srflx candidates
   - Return sorted candidate list by priority
2. Add configurable STUN server list

**API:**
```rust
pub struct NatConfig {
    pub stun_servers: Vec<String>,
    pub candidate_timeout: Duration,
    pub prefer_ipv6: bool,
}

pub async fn gather_candidates(config: &NatConfig) -> Result<Vec<Candidate>, NatError>;
```

**PR:** `feat(nat): add candidate gathering with STUN`

#### 2.3: Add Tests
**File:** `crates/rift-nat/src/stun.rs`

**Tests:**
1. STUN Binding Request encoding/decoding
2. Public address discovery (requires STUN server or mock)
3. Multiple server fallback
4. Timeout handling
5. Candidate priority calculation

**Use public STUN servers for integration tests:**
- `stun.l.google.com:19302`
- `stun1.l.google.com:19302`

**PR:** `test(nat): add STUN client tests`

#### 2.4: Update Configuration
**Files:**
- `crates/rift-sdk/src/lib.rs`
- Example config in README

Add STUN server configuration:
```toml
[network]
stun_servers = [
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302"
]
```

**PR:** `feat(sdk): expose STUN configuration`

#### 2.5: Enable E2E Test
**File:** `tests/e2e/mod.rs`

Remove `#[ignore]` from:
```rust
#[test]
fn stun_srflx_connectivity()
```

Implement test body:
```rust
#[tokio::test]
async fn stun_srflx_connectivity() {
    let config = NatConfig {
        stun_servers: vec!["stun.l.google.com:19302".to_string()],
        candidate_timeout: Duration::from_secs(5),
        prefer_ipv6: false,
    };
    
    let candidates = gather_candidates(&config).await.unwrap();
    
    // Assert we got at least one srflx candidate
    assert!(candidates.iter().any(|c| matches!(c.typ, CandidateType::ServerReflexive)));
}
```

**PR:** `test(e2e): enable STUN connectivity test`

**Deliverables:**
- [ ] STUN client implemented and tested
- [ ] Candidate gathering working
- [ ] Integration test passing
- [ ] Configuration exposed in SDK

**Time:** 2-3 weeks

---

## Phase 3: ICE-lite Protocol Updates (Phase 35.2) (Week 7-8) 📡

**Goal:** Add ICE candidate exchange messages to protocol.

### Background
`rift-protocol` needs new message types for ICE candidate exchange and connectivity checks.

### Tasks

#### 3.1: Add ICE Message Types
**File:** `crates/rift-protocol/src/control.rs`

**Add to `ControlMessage` enum:**
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ControlMessage {
    // ... existing variants ...
    
    /// Send ICE candidates to peer
    IceCandidates {
        session_id: SessionId,
        candidates: Vec<IceCandidate>,
    },
    
    /// Request connectivity check
    IceCheck {
        session_id: SessionId,
        candidate_pair: CandidatePair,
        priority: u32,
    },
    
    /// Acknowledge connectivity check
    IceCheckAck {
        session_id: SessionId,
        candidate_pair: CandidatePair,
        success: bool,
    },
    
    /// Signal selected candidate pair
    IceSelectedPair {
        session_id: SessionId,
        candidate_pair: CandidatePair,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IceCandidate {
    pub typ: CandidateType,
    pub addr: SocketAddr,
    pub priority: u32,
    pub foundation: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CandidatePair {
    pub local: IceCandidate,
    pub remote: IceCandidate,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum CandidateType {
    Host,
    ServerReflexive,
    Relayed,
}
```

**PR:** `feat(protocol): add ICE message types (Phase 35.2)`

#### 3.2: Update Hello Message
**File:** `crates/rift-protocol/src/control.rs`

**Update `Hello` variant:**
```rust
Hello {
    peer_id: PeerId,
    protocol_version: u8,
    capabilities: Vec<String>,
    initial_candidates: Vec<IceCandidate>, // NEW
},
```

**PR:** `feat(protocol): include candidates in Hello message`

#### 3.3: Add Protocol Tests
**File:** `crates/rift-protocol/src/tests.rs`

**Add tests:**
1. ICE candidate serialization roundtrip
2. IceCheck message encoding/decoding
3. Hello with candidates
4. Backward compatibility (empty candidates for old clients)

**PR:** `test(protocol): add ICE message tests`

#### 3.4: Update Protocol Documentation
**File:** `docs/PROTOCOL.md`

Document new message types:
```markdown
### ICE Messages

#### IceCandidates
Sent after connection to exchange discovered candidates.
Fields: session_id, candidates (array)

#### IceCheck
Request connectivity check on a candidate pair.
Fields: session_id, candidate_pair, priority

#### IceCheckAck
Response to connectivity check.
Fields: session_id, candidate_pair, success (bool)

#### IceSelectedPair
Signal the selected candidate pair after checks complete.
Fields: session_id, candidate_pair
```

**PR:** `docs: document ICE protocol messages`

**Deliverables:**
- [ ] ICE message types in protocol
- [ ] Hello message includes candidates
- [ ] Protocol tests passing
- [ ] Documentation updated

**Time:** 1-2 weeks

---

## Phase 4: ICE-lite Connectivity Checks (Phase 35.3) (Week 9-12) 🔌

**Goal:** Implement ICE-lite connectivity checks and path selection.

### Background
`rift-mesh` needs to perform connectivity checks on candidate pairs and select the best path.

### Tasks

#### 4.1: Implement Connectivity Checker
**File:** `crates/rift-mesh/src/ice.rs` (new)

**Steps:**
1. Pair local and remote candidates
2. Sort pairs by priority
3. Send IceCheck on each pair
4. Await IceCheckAck with timeout
5. Mark successful pairs as valid

**API:**
```rust
pub struct IceChecker {
    local_candidates: Vec<Candidate>,
    remote_candidates: Vec<Candidate>,
    check_timeout: Duration,
}

impl IceChecker {
    pub fn new(local: Vec<Candidate>, remote: Vec<Candidate>) -> Self;
    
    pub async fn run_checks(&mut self, socket: &UdpSocket) 
        -> Result<Vec<ValidCandidatePair>, IceError>;
    
    pub fn select_best_pair(&self, valid_pairs: &[ValidCandidatePair]) 
        -> Option<ValidCandidatePair>;
}

pub struct ValidCandidatePair {
    pub local: Candidate,
    pub remote: Candidate,
    pub rtt: Duration,
    pub priority: u32,
}
```

**Priority calculation:**
```rust
fn calculate_pair_priority(local: &Candidate, remote: &Candidate) -> u32 {
    // ICE-lite: prefer direct > srflx > relayed
    match (local.typ, remote.typ) {
        (CandidateType::Host, CandidateType::Host) => 1000,
        (CandidateType::ServerReflexive, _) | (_, CandidateType::ServerReflexive) => 500,
        (CandidateType::Relayed, _) | (_, CandidateType::Relayed) => 100,
    }
}
```

**PR:** `feat(mesh): implement ICE-lite connectivity checker (Phase 35.3)`

#### 4.2: Integrate into Session Establishment
**File:** `crates/rift-mesh/src/session.rs`

**Steps:**
1. After receiving `Hello`, gather local candidates
2. Send `IceCandidates` to peer
3. Upon receiving peer's `IceCandidates`, run connectivity checks
4. Select best pair and send `IceSelectedPair`
5. Switch to selected candidate pair for data transmission

**Sequence:**
```
Peer A                           Peer B
  |                                |
  |-- Hello (with candidates) ---->|
  |<--- Hello (with candidates) ---|
  |                                |
  |-- IceCandidates (if more) ---->|
  |<--- IceCandidates -------------|
  |                                |
  |-- IceCheck (pair 1) ---------->|
  |<--- IceCheckAck (success) -----|
  |                                |
  |-- IceSelectedPair ------------>|
  |<--- IceSelectedPair ------------|
  |                                |
  |====== Data on best path =======|
```

**PR:** `feat(mesh): integrate ICE checks into session setup`

#### 4.3: Implement Keep-Alives
**File:** `crates/rift-mesh/src/ice.rs`

**Steps:**
1. After path selection, start periodic keep-alive timer (e.g., every 15 seconds)
2. Send small ping packet on selected path
3. If no response after 3 attempts, trigger path reselection
4. Log path transitions for debugging

**API:**
```rust
pub struct PathKeepAlive {
    interval: Duration,
    max_failures: usize,
}

impl PathKeepAlive {
    pub async fn run(&mut self, socket: &UdpSocket, peer_addr: SocketAddr) 
        -> Result<(), IceError>;
}
```

**PR:** `feat(mesh): add ICE path keep-alives`

#### 4.4: Add Path Switching
**File:** `crates/rift-mesh/src/ice.rs`

**Steps:**
1. Monitor packet loss / RTT on active path
2. If quality degrades, re-run connectivity checks
3. Switch to better path if found
4. Notify peer via `IceSelectedPair`

**Triggers:**
- Packet loss > 10% over 10 seconds
- RTT increases > 2x baseline
- Keep-alive failures

**PR:** `feat(mesh): implement dynamic path switching`

#### 4.5: Add Tests
**Files:**
- `crates/rift-mesh/src/ice.rs`
- `tests/e2e/mod.rs`

**Unit tests:**
1. Candidate pairing and prioritization
2. Connectivity check roundtrip
3. Path selection logic
4. Keep-alive timer

**Integration test:**
```rust
#[tokio::test]
async fn ice_lite_path_selection() {
    // Setup two peers with multiple candidates
    let peer_a = setup_peer_with_candidates().await;
    let peer_b = setup_peer_with_candidates().await;
    
    // Run ICE checks
    let pair = peer_a.connect_with_ice(peer_b.addr()).await.unwrap();
    
    // Assert best path selected (e.g., direct if available)
    assert_eq!(pair.local.typ, CandidateType::Host);
}
```

**Enable ignored test:**
```rust
#[tokio::test]
async fn nat_restrictive_turn_fallback() {
    // Requires TURN server setup
    // Test that ICE falls back to TURN when direct/srflx fail
}
```

**PR:** `test(mesh): add ICE connectivity tests`

**Deliverables:**
- [ ] ICE-lite checker implemented
- [ ] Integrated into session setup
- [ ] Keep-alives working
- [ ] Dynamic path switching
- [ ] Tests passing

**Time:** 3-4 weeks

---

## Phase 5: SDK Integration and UI (Phase 35.4) (Week 13-14) 🎨

**Goal:** Expose ICE functionality in SDK and clients.

### Tasks

#### 5.1: Add ICE Configuration to SDK
**File:** `crates/rift-sdk/src/config.rs`

**Add fields:**
```rust
pub struct NetworkConfig {
    pub stun_servers: Vec<String>,
    pub enable_ice: bool,
    pub ice_check_timeout: Duration,
    pub keep_alive_interval: Duration,
}
```

**Default config:**
```rust
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
            ],
            enable_ice: true,
            ice_check_timeout: Duration::from_secs(5),
            keep_alive_interval: Duration::from_secs(15),
        }
    }
}
```

**PR:** `feat(sdk): add ICE configuration`

#### 5.2: Expose Path Statistics
**File:** `crates/rift-sdk/src/session.rs`

**Add path info API:**
```rust
pub struct PathInfo {
    pub candidate_type: CandidateType,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub rtt: Duration,
    pub packet_loss: f64,
}

impl RiftSession {
    pub fn get_path_info(&self) -> Option<PathInfo>;
    pub fn force_path_recheck(&mut self);
}
```

**PR:** `feat(sdk): expose path statistics API`

#### 5.3: Update TUI Status Bar
**File:** `bin/rift/src/ui.rs`

**Add to status bar:**
```
Channel: gaming (3 peers) | Mic: On | Quality: Medium | Call: Active | Path: Direct (15ms)
```

Show path type and RTT for active connections.

**PR:** `feat(tui): show ICE path info in status bar`

#### 5.4: Update Qt Client
**Files:**
- `clients/rift-qt-unified/src/rift_bridge.cpp`
- `clients/rift-qt-unified/qml/PeerList.qml`

**Add path indicator to peer list:**
- 🟢 Direct connection
- 🟡 Via STUN (srflx)
- 🔴 Via relay

**PR:** `feat(qt): show connection path in peer list`

#### 5.5: Update Documentation
**Files:**
- `README.md`
- `docs/PHASE35_PLAN.md`
- Example config files

**Update README:**
```markdown
- NAT traversal: UDP hole punching + STUN candidates + ICE-lite path selection + optional TURN fallback.
```

**Mark Phase 35 as complete:**
```markdown
## Phase 35 Status: ✅ Complete
- Phase 35.1: STUN client + candidate gathering ✅
- Phase 35.2: Protocol updates + candidate exchange ✅
- Phase 35.3: ICE-lite checks + path selection + keep-alives ✅
- Phase 35.4: SDK/config exposure + client status UI ✅
```

**PR:** `docs: update for Phase 35 completion`

**Deliverables:**
- [ ] ICE config in SDK
- [ ] Path statistics API
- [ ] TUI shows path info
- [ ] Qt shows connection indicators
- [ ] Documentation updated

**Time:** 1-2 weeks

---

## Phase 6: Browser WebRTC Integration (Week 15-18) 🌐

**Goal:** Add WebRTC transport to browser client for true P2P.

### Background
Current browser client is WebSocket-only. This phase adds WebRTC data channels and ICE for NAT traversal.

### Tasks

#### 6.1: Add WebRTC Transport to rift-wasm
**File:** `crates/rift-wasm/src/transport.rs` (new)

**Steps:**
1. Wrap browser WebRTC API (RTCPeerConnection, RTCDataChannel)
2. Implement ICE candidate exchange via signaling channel
3. Support STUN/TURN configuration
4. Multiplex data channels for different message types

**WASM bindings:**
```rust
#[wasm_bindgen]
pub struct WebRtcTransport {
    peer_connection: web_sys::RtcPeerConnection,
    data_channel: Option<web_sys::RtcDataChannel>,
}

#[wasm_bindgen]
impl WebRtcTransport {
    pub fn new(stun_servers: Vec<String>) -> Result<WebRtcTransport, JsValue>;
    pub async fn create_offer(&self) -> Result<String, JsValue>;
    pub async fn set_remote_answer(&self, sdp: &str) -> Result<(), JsValue>;
    pub fn on_ice_candidate(&self, callback: js_sys::Function);
    pub fn send(&self, data: &[u8]) -> Result<(), JsValue>;
    pub fn on_message(&self, callback: js_sys::Function);
}
```

**Dependencies:**
```toml
[dependencies]
web-sys = { version = "0.3", features = ["RtcPeerConnection", "RtcDataChannel", "RtcIceServer"] }
```

**PR:** `feat(wasm): add WebRTC transport`

#### 6.2: Hybrid Transport in rift-web-chat
**File:** `crates/rift-web-chat/src/lib.rs`

**Strategy:**
1. Start with WebSocket (signaling + fallback)
2. Exchange WebRTC offers/answers via WebSocket
3. Establish data channel
4. Switch to data channel for data
5. Keep WebSocket alive for signaling

**API stays same, transport is internal:**
```rust
pub struct WebChat {
    ws_transport: WebSocketTransport,
    rtc_transport: Option<WebRtcTransport>,
    mode: TransportMode,
}

enum TransportMode {
    WebSocketOnly,
    HybridSignaling,
    DataChannelActive,
}
```

**PR:** `feat(web-chat): add hybrid WebSocket/WebRTC transport`

#### 6.3: Add WebRTC Audio
**File:** `crates/rift-wasm/src/audio.rs` (new)

**Steps:**
1. Wrap getUserMedia() for mic access
2. Wrap AudioContext for playback
3. Add Opus encoding/decoding (via WASM)
4. Stream audio over WebRTC data channel

**WASM bindings:**
```rust
#[wasm_bindgen]
pub struct AudioCapture {
    stream: web_sys::MediaStream,
    encoder: OpusEncoder,
}

#[wasm_bindgen]
impl AudioCapture {
    pub async fn new(constraints: JsValue) -> Result<AudioCapture, JsValue>;
    pub fn read_frame(&mut self) -> Option<Vec<u8>>;
}

#[wasm_bindgen]
pub struct AudioPlayback {
    context: web_sys::AudioContext,
    decoder: OpusDecoder,
}

#[wasm_bindgen]
impl AudioPlayback {
    pub fn new() -> Result<AudioPlayback, JsValue>;
    pub fn play_frame(&mut self, opus_data: &[u8]) -> Result<(), JsValue>;
}
```

**Dependencies:**
```toml
[dependencies]
audiopus = "0.2" # WASM-compatible Opus bindings
```

**PR:** `feat(wasm): add WebRTC audio capture/playback`

#### 6.4: Update Browser Demo
**File:** `www/index.html`

**Add:**
- WebRTC toggle button
- Audio toggle button
- Connection status indicator (WebSocket vs WebRTC)
- Mic/speaker device selection

**PR:** `feat(www): add WebRTC and audio UI`

#### 6.5: Update Documentation
**File:** `docs/README.browser.md`

**Replace limitations section:**
```markdown
## Features

- ✅ Text chat with pairwise E2EE
- ✅ WebRTC P2P connectivity (no relay required)
- ✅ Voice calls with Opus codec
- ✅ STUN/TURN support for NAT traversal
- ⚠️ UDP mesh discovery not supported (browser sandbox limitation)
```

**PR:** `docs: update browser client capabilities`

**Deliverables:**
- [ ] WebRTC transport in WASM
- [ ] Hybrid transport in web-chat
- [ ] Audio working in browser
- [ ] Updated demo and docs

**Time:** 3-4 weeks

---

## Phase 7: Protocol Hardening (Week 19-20) 🛡️

**Goal:** Implement capability negotiation and version handling.

### Tasks

#### 7.1: Capability Negotiation
**File:** `crates/rift-protocol/src/capability.rs` (new)

**Design:**
```rust
pub enum Capability {
    Ice,
    Relay,
    VoiceOpus,
    VoiceOpusHr, // high-res
    MultiChannel,
    VideoH264,
    VideoVp9,
}

pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn negotiate(&self, remote: &CapabilitySet) -> CapabilitySet;
}
```

**Update Hello:**
```rust
Hello {
    peer_id: PeerId,
    protocol_version: u8,
    capabilities: CapabilitySet,
    initial_candidates: Vec<IceCandidate>,
}
```

**PR:** `feat(protocol): add capability negotiation`

#### 7.2: Version Negotiation
**File:** `crates/rift-protocol/src/version.rs` (new)

**Design:**
```rust
pub const CURRENT_VERSION: u8 = 1;
pub const MIN_SUPPORTED_VERSION: u8 = 1;

pub fn negotiate_version(local: u8, remote: u8) -> Option<u8> {
    let min = local.min(remote);
    if min >= MIN_SUPPORTED_VERSION {
        Some(min)
    } else {
        None
    }
}
```

**Handle version mismatch:**
```rust
if let Some(version) = negotiate_version(CURRENT_VERSION, peer_version) {
    // Use negotiated version
    session.set_protocol_version(version);
} else {
    return Err(ProtocolError::VersionMismatch);
}
```

**PR:** `feat(protocol): add version negotiation`

#### 7.3: Formal Error Types
**File:** `crates/rift-protocol/src/control.rs`

**Add to ControlMessage:**
```rust
Error {
    code: ErrorCode,
    message: String,
}

Ack {
    sequence: u64,
}
```

**Error codes:**
```rust
pub enum ErrorCode {
    VersionMismatch = 1,
    UnsupportedCapability = 2,
    AuthenticationFailed = 3,
    SessionClosed = 4,
    RateLimitExceeded = 5,
}
```

**PR:** `feat(protocol): add error/ack messages`

#### 7.4: Update Documentation
**File:** `docs/PROTOCOL.md`

Document:
- Capability negotiation flow
- Version negotiation process
- Error codes and handling

**PR:** `docs: document protocol hardening features`

**Deliverables:**
- [ ] Capability negotiation
- [ ] Version handling
- [ ] Error types
- [ ] Documentation

**Time:** 1-2 weeks

---

## Phase 8: Integration Tests and Validation (Week 21-22) ✅

**Goal:** Comprehensive testing of all new features.

### Tasks

#### 8.1: LAN Integration Tests
**File:** `tests/e2e/lan.rs` (new)

**Tests:**
1. mDNS peer discovery
2. Direct connection establishment
3. Text message roundtrip
4. Voice call setup and teardown

**PR:** `test(e2e): add LAN integration tests`

#### 8.2: Internet Integration Tests
**File:** `tests/e2e/internet.rs` (new)

**Tests:**
1. STUN candidate gathering
2. ICE connectivity checks
3. TURN relay fallback (requires TURN server)
4. DHT peer discovery
5. Invite-based join

**PR:** `test(e2e): add internet integration tests`

#### 8.3: Browser Integration Tests
**File:** `tests/e2e/browser.rs` (new)

**Tests (using headless browser):**
1. WebSocket connection
2. WebRTC data channel
3. Text chat roundtrip
4. Noise handshake
5. Audio capture/playback

**PR:** `test(e2e): add browser integration tests`

#### 8.4: Stress Tests
**File:** `tests/stress/` (new directory)

**Tests:**
1. 100 peers in mesh
2. High packet loss simulation
3. NAT type permutations
4. Rapid connect/disconnect cycles

**PR:** `test(stress): add stress and reliability tests`

#### 8.5: Documentation Tests
**File:** `tests/doc_tests.rs`

Ensure all code examples in docs compile and run:
- README.md examples
- docs/*.md examples
- Crate-level docs

**PR:** `test: validate documentation examples`

**Deliverables:**
- [ ] Integration tests for LAN, internet, browser
- [ ] Stress tests
- [ ] Doc tests
- [ ] CI passing all tests

**Time:** 1-2 weeks

---

## Phase 9: Production Readiness (Week 23-24) 🚀

**Goal:** Final polish for production deployment.

### Tasks

#### 9.1: Performance Audit
**Tool:** `cargo bench`

**Benchmark:**
1. Protocol encode/decode throughput
2. Crypto overhead (E2EE, handshake)
3. ICE check latency
4. Audio codec latency
5. Memory usage under load

**Document in:** `docs/OPTIMIZATION_REPORT.md`

**PR:** `perf: benchmark and optimize critical paths`

#### 9.2: Security Audit
**Tool:** `cargo audit`, manual review

**Check:**
1. No panics in network-facing code
2. Input validation on all external data
3. Rate limiting on expensive operations
4. DoS resistance (e.g., probe floods)
5. Dependency vulnerabilities

**Document in:** `docs/SECURITY.md`

**PR:** `security: audit and harden against threats`

#### 9.3: Client Polish

**TUI:**
- Add help screen (F1)
- Improve error messages
- Add connection diagnostics command

**Qt:**
- Add settings dialog for ICE config
- Show path info tooltip on hover
- Add connection troubleshooting page

**Android:**
- Add ICE stats in debug menu
- Improve error notifications

**PR:** `feat(clients): polish and UX improvements`

#### 9.4: Release Preparation
**Files:**
- `CHANGELOG.md`
- `docs/RELEASE_CHECKLIST.md`
- Version bumps in `Cargo.toml` files

**PR:** `chore: prepare v0.2.0 release`

**Deliverables:**
- [ ] Performance benchmarks
- [ ] Security audit complete
- [ ] Clients polished
- [ ] Release artifacts ready

**Time:** 1-2 weeks

---

## Summary Timeline

| Phase | Description | Duration | Deliverables |
|-------|-------------|----------|--------------|
| 0 | Cleanup and Documentation | 1 week | 4 PRs, accurate docs |
| 1 | Browser Security Fix | 2 weeks | Noise handshake in WASM |
| 2 | STUN Client (35.1) | 3 weeks | STUN working, tests pass |
| 3 | ICE Protocol (35.2) | 2 weeks | ICE messages in protocol |
| 4 | ICE Checks (35.3) | 4 weeks | Connectivity checks working |
| 5 | SDK Integration (35.4) | 2 weeks | ICE in SDK/TUI/Qt |
| 6 | Browser WebRTC | 4 weeks | P2P browser client |
| 7 | Protocol Hardening | 2 weeks | Capabilities, versioning |
| 8 | Integration Tests | 2 weeks | Full test coverage |
| 9 | Production Readiness | 2 weeks | Performance, security |

**Total:** 22-24 weeks (~5-6 months)

---

## PR Estimate by Category

| Category | Estimated PRs |
|----------|--------------|
| Cleanup & Docs | 4 |
| Browser Security | 4 |
| STUN/ICE (Phase 35) | 8-10 |
| Browser WebRTC | 5 |
| Protocol Hardening | 4 |
| Testing | 5 |
| Production Polish | 3 |
| **Total** | **33-38 PRs** |

---

## Risk Mitigation

### Risks

1. **STUN/ICE Complexity** - ICE is notoriously tricky
   - Mitigation: Start with ICE-lite (simpler), test thoroughly, reference RFCs

2. **Browser WebRTC Compatibility** - Browser APIs vary
   - Mitigation: Test on Chrome, Firefox, Safari; polyfill where needed

3. **Relay Server Costs** - TURN servers are expensive
   - Mitigation: Make TURN optional, document self-hosting, use STUN primarily

4. **Performance Degradation** - Adding features may slow things down
   - Mitigation: Benchmark each phase, optimize hot paths

### Dependencies

- **Phase 35 must complete before browser WebRTC** - browser needs ICE support
- **Security fix (Phase 1) should complete early** - security is critical
- **Protocol hardening can happen in parallel with ICE** - independent work

---

## Success Criteria

At completion of this plan, riftd should have:

✅ **Security:**
- Pairwise E2EE in all clients (including browser)
- No shared keys, proper Noise handshakes
- Security audit passed

✅ **Connectivity:**
- STUN candidate gathering working
- ICE-lite connectivity checks functional
- Dynamic path selection and switching
- TURN fallback (optional)

✅ **Browser:**
- WebRTC P2P support (no relay required)
- Voice calls working
- Feature parity with native clients (where possible)

✅ **Testing:**
- 90%+ unit test coverage on new code
- Integration tests for LAN and internet
- Browser tests via headless automation
- Stress tests for reliability

✅ **Documentation:**
- All docs accurate and up-to-date
- Feature status matrix maintained
- Protocol spec complete
- Examples working

---

## Post-Plan Work (Future)

After this plan completes, consider:

1. **rndzv 2.0** - Multi-party rendezvous, routing, naming (Phases 3-10)
2. **Group Calls** - Multi-party voice with SFU/MCU
3. **Video Support** - H.264/VP9 codec integration
4. **Mobile Optimization** - Battery, bandwidth, backgrounding
5. **Multi-Device Sync** - Identity and state across devices

---

## Conclusion

This implementation plan systematically addresses all gaps identified in the audit:

- Removes misleading "stub" comments
- Fixes browser security issue
- Completes Phase 35 (STUN/ICE-lite)
- Brings browser to feature parity
- Hardens protocol
- Achieves production readiness

**Estimated Timeline:** 5-6 months  
**Estimated Effort:** 33-38 PRs  
**Result:** Production-ready P2P communication stack with full NAT traversal and browser support.
