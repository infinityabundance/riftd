# riftd Feature Matrix: Working vs Claimed vs Planned

**Quick visual reference for what works, what's claimed, and what's planned**

---

## Legend

- ✅ **Working** - Implemented and tested
- ⚠️ **Partial** - Implemented but incomplete/limited
- 🚧 **In Progress** - Implementation started
- 📋 **Planned** - Documented plan exists
- ❌ **Not Started** - No implementation
- 🔴 **Security Issue** - Has known security problem

---

## Core Networking

| Feature | Native Clients | Browser Client | Claimed in Docs | Notes |
|---------|---------------|----------------|-----------------|-------|
| **P2P Mesh Topology** | ✅ Working | ❌ WebSocket only | ✅ Yes | Browser needs WebRTC |
| **LAN mDNS Discovery** | ✅ Working | ❌ N/A | ✅ Yes | Browser sandbox limitation |
| **DHT Discovery** | ✅ Working | ❌ Not integrated | ✅ Yes | |
| **UDP Hole Punching** | ✅ Working | ❌ N/A | ✅ Yes | Browser needs WebRTC |
| **STUN Candidates** | ❌ Not started | ❌ Not started | ⚠️ Yes (misleading) | Phase 35.1 |
| **ICE-lite Checks** | ❌ Not started | ❌ Not started | ⚠️ Yes (misleading) | Phase 35.2-35.3 |
| **TURN Relay** | ⚠️ Client exists | ❌ Not started | ✅ Yes (optional) | Not integrated with ICE |
| **Relay Fallback** | ✅ Working | ❌ No P2P | ✅ Yes | Peer relay, not TURN |

**Summary:** Basic connectivity works, advanced NAT traversal (STUN/ICE) not implemented.

---

## Security & Encryption

| Feature | Native Clients | Browser Client | Claimed in Docs | Notes |
|---------|---------------|----------------|-----------------|-------|
| **Pairwise E2EE** | ✅ Working | 🔴 Shared key | ✅ Yes | Browser security issue |
| **Noise Protocol** | ✅ Working | 🔴 Not used | ✅ Yes | Browser uses channel key |
| **Identity (Ed25519)** | ✅ Working | ✅ Working | ✅ Yes | |
| **Key Exchange** | ✅ Working | 🔴 No handshake | ✅ Yes | Browser needs Noise handshake |
| **Invite Signatures** | ✅ Working | ✅ Working | ✅ Yes | |

**Summary:** Native clients secure, browser has critical security gap.

---

## Media & Communication

| Feature | Native Clients | Browser Client | Claimed in Docs | Notes |
|---------|---------------|----------------|-----------------|-------|
| **Text Chat** | ✅ Working | ✅ Working | ✅ Yes | |
| **Voice (Opus)** | ✅ Working | ❌ Not started | ⚠️ Native only | Browser needs audio |
| **Quality Presets** | ✅ Working | ❌ N/A | ✅ Yes | Low/med/high |
| **VAD (Voice Activity)** | ✅ Working | ❌ N/A | ✅ Yes | |
| **Push-to-Talk** | ✅ Working | ❌ N/A | ✅ Yes | |
| **Mute/Unmute** | ✅ Working | ❌ N/A | ✅ Yes | |
| **Call Management** | ✅ Working | ❌ N/A | ✅ Yes | /call, /hangup commands |
| **Group Calls** | ❌ Not started | ❌ Not started | 📋 Roadmap | |

**Summary:** Voice works great on native, browser is text-only.

---

## Protocol Features

| Feature | Status | Claimed in Docs | Notes |
|---------|--------|-----------------|-------|
| **Versioned Framing** | ✅ Working | ✅ Yes | |
| **Control Messages** | ✅ Working | ✅ Yes | Hello, Relay, Call, etc. |
| **Capability Negotiation** | ❌ Not started | 📋 Roadmap | Phase 7 |
| **Version Negotiation** | ❌ Not started | 📋 Roadmap | Phase 7 |
| **Error/Ack Messages** | ⚠️ Partial | 📋 Roadmap | Phase 7 |
| **ICE Messages** | ❌ Not started | 📋 Phase 35.2 | IceCandidates, IceCheck, etc. |

**Summary:** Core protocol works, advanced features planned.

---

## Predictive Rendezvous

| Feature | Status | Claimed in Docs | Notes |
|---------|--------|-----------------|-------|
| **SRT Generation** | ✅ Working | ✅ Yes | |
| **Schedule Derivation** | ✅ Working | ✅ Yes | |
| **Probe/Response** | ✅ Working | ✅ Yes | |
| **Session Establishment** | ✅ Working | ✅ Yes | |
| **Channel Multiplexing** | ✅ Working | ✅ Yes | |
| **Metrics Collection** | ✅ Working | ✅ Yes | |
| **rndzv 2.0 (Multi-party)** | ❌ Not started | 📋 Plan exists | Phases 3-10 |

**Summary:** Phase 1-2 complete and working, 2.0 is future work.

---

## Clients

| Client | Platform | Status | Features | Notes |
|--------|----------|--------|----------|-------|
| **TUI** | Linux/macOS/Windows | ✅ Working | Full | Terminal UI with Ratatui |
| **Qt Desktop** | Linux/macOS/Windows | ✅ Working | Full | Qt6/QML with C FFI |
| **Android** | Android | ✅ Working | Full | Kotlin/Compose with JNI |
| **Browser** | Web | ⚠️ Partial | Text only | Security issue, no P2P/audio |

**Client Feature Comparison:**

| Feature | TUI | Qt | Android | Browser |
|---------|-----|----|---------|---------| 
| Text Chat | ✅ | ✅ | ✅ | ✅ |
| Voice Calls | ✅ | ✅ | ✅ | ❌ |
| P2P Mesh | ✅ | ✅ | ✅ | ❌ |
| Pairwise E2EE | ✅ | ✅ | ✅ | 🔴 |
| LAN Discovery | ✅ | ✅ | ✅ | ❌ |
| Invite Join | ✅ | ✅ | ✅ | ✅ |
| System Tray | ❌ | ✅ | ❌ | ❌ |
| Background Service | ❌ | ❌ | ✅ | ❌ |

**Summary:** Native clients feature-complete, browser needs work.

---

## Crates Status

| Crate | Purpose | Status | Test Coverage |
|-------|---------|--------|---------------|
| `rift-core` | Identity, crypto | ✅ Complete | ✅ 3/3 pass |
| `rift-protocol` | Wire protocol | ✅ Complete | ✅ 2/2 pass |
| `rift-nat` | NAT traversal | ⚠️ Partial | ✅ 4/4 pass |
| `rift-rndzv` | Rendezvous | ✅ Complete | ✅ 22/22 pass |
| `rift-discovery` | mDNS | ✅ Working | ⚠️ No tests |
| `rift-dht` | DHT | ✅ Working | ⚠️ No tests |
| `rift-mesh` | Mesh routing | ✅ Working | ⚠️ No tests |
| `rift-media` | Audio I/O | ✅ Working | ⚠️ No tests |
| `rift-torrent` | File sharing | ✅ Complete | ✅ 27/27 pass |
| `rift-wasm` | WASM bindings | ⚠️ Partial | ⚠️ No tests |
| `rift-web-chat` | Browser client | ⚠️ Partial | ⚠️ No tests |
| `rift-sdk` | High-level API | ✅ Working | ⚠️ No tests |
| `rift-metrics` | Telemetry | ✅ Complete | ✅ Builds |
| `rift-e2e` | E2E tests | ✅ Working | ⚠️ 2 ignored |

**Summary:** Core crates solid, need integration tests.

---

## Phase 35 Status (STUN/ICE-lite)

| Phase | Task | Status | Estimated Effort |
|-------|------|--------|------------------|
| **35.1** | STUN client + candidates | ❌ 0% | 2-3 weeks |
| **35.2** | Protocol ICE messages | ❌ 0% | 1-2 weeks |
| **35.3** | Connectivity checks | ❌ 0% | 3-4 weeks |
| **35.4** | SDK/UI integration | ❌ 0% | 1-2 weeks |

**Total:** 0% complete, 7-11 weeks of work

---

## Implementation Priorities

### 🔴 Critical (Security)
1. Fix browser E2EE (implement Noise handshake)
2. Security audit of network-facing code
3. Remove misleading documentation

### 🟠 High (Core Functionality)
1. Complete Phase 35.1 (STUN client)
2. Complete Phase 35.2-35.3 (ICE-lite)
3. Add integration tests

### 🟡 Medium (Feature Parity)
1. Browser WebRTC integration
2. Browser audio support
3. Qt invite parsing

### 🟢 Low (Enhancement)
1. Protocol hardening (capabilities, versioning)
2. Group call support
3. rndzv 2.0 multi-party

---

## Documentation Accuracy

| Document | Sections | Accurate | Misleading | Outdated |
|----------|----------|----------|------------|----------|
| `README.md` | 8 | 7 | 1 | 0 |
| `docs/ROADMAP.md` | 3 | 3 | 0 | 0 |
| `docs/PHASE35_PLAN.md` | 4 | 4 | 0 | 0 |
| `docs/README.browser.md` | 3 | 3 | 0 | 0 |

**Misleading Claims:**
- README claims "STUN candidates" work (they don't)
- Implies Phase 35 is complete (it's not started)

**Recommendation:** Add "Status: Planned" to STUN/ICE mentions in README.

---

## Test Coverage Summary

### Unit Tests
- **Passing:** 58/58 (100%)
- **Ignored:** 2 (require infrastructure)
- **Coverage:** Core crypto/protocol only

### Integration Tests
- **Passing:** 0 (none exist)
- **Needed:** LAN discovery, DHT, mesh routing, relay

### E2E Tests
- **Passing:** 0 (stubs only)
- **Ignored:** 2 (TURN/STUN)
- **Needed:** Full flow tests

**Overall Test Health:** 🟡 Unit tests strong, integration tests missing

---

## Comparison: What You Can Do Today

### ✅ Works Today (Native Clients)
```bash
# LAN voice chat
cargo run -p rift -- create --channel gaming --voice --port 7777
# In another terminal:
XDG_CONFIG_HOME=/tmp/rift2 cargo run -p rift -- create --channel gaming --voice --port 7778
# Peers auto-discover via mDNS and connect
# Type messages, use /call to voice chat
```

### ⚠️ Partially Works (Browser)
```bash
# Text-only chat via relay
cargo run -p rift-ws-relay
wasm-pack build crates/rift-web-chat --target web
python3 -m http.server --directory www 8080
# Open two browser tabs, paste same invite
# Can text chat, but relay can decrypt (insecure)
```

### ❌ Doesn't Work Yet
```bash
# STUN candidate gathering
cargo run -p rift -- create --channel test --stun-servers stun.l.google.com:19302
# ^ This flag doesn't exist because STUN isn't implemented

# Browser P2P (no relay)
# ^ Impossible, needs WebRTC integration

# Browser voice
# ^ Not implemented
```

---

## When Will X Be Ready?

Based on `IMPLEMENTATION_PLAN.md`:

| Feature | Estimated Ready | Weeks from Now |
|---------|----------------|----------------|
| **Browser E2EE Fix** | Week 3 | 2-3 weeks |
| **STUN Client** | Week 6 | 5-6 weeks |
| **ICE-lite Complete** | Week 12 | 11-12 weeks |
| **Browser WebRTC** | Week 18 | 17-18 weeks |
| **Protocol Hardening** | Week 20 | 19-20 weeks |
| **Production Ready** | Week 24 | 23-24 weeks |

**Timeline:** ~6 months to complete all planned work.

---

## What to Read Next

1. **For quick overview:** This file
2. **For detailed findings:** `AUDIT_REPORT.md`
3. **For implementation guide:** `IMPLEMENTATION_PLAN.md`
4. **For action items:** `GAP_ANALYSIS.md`

---

## One-Sentence Summary

**Native P2P voice+text mesh works great (✅), STUN/ICE not implemented (❌), browser client insecure and limited (🔴), ~6 months to production-ready (📅).**
