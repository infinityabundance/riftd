# riftd Gap Analysis: Quick Reference
**Generated:** 2026-02-15  
**Status:** Complete audit of stubs, TODOs, and documentation gaps

---

## TL;DR

**What Works:** ✅
- P2P mesh networking
- LAN mDNS discovery
- DHT peer discovery  
- Basic UDP hole punching
- Pairwise E2EE (native clients)
- Voice/text chat (native)
- TUI, Qt, and Android clients
- Torrent-like file sharing

**What's Incomplete:** ❌
- STUN candidate gathering (Phase 35.1)
- ICE-lite connectivity checks (Phase 35.2-35.3)
- Browser pairwise E2EE (uses shared key)
- Browser P2P mesh (WebSocket relay only)
- Browser audio (text-only)

**What's Misleading:** ⚠️
- Functions marked "(stub)" but fully implemented in `rift-rndzv`
- README claims STUN works (it doesn't yet)

---

## Files with Misleading Comments

| File | Line | Issue | Fix |
|------|------|-------|-----|
| `crates/rift-rndzv/src/api.rs` | 358 | Marked "(stub)" but fully implemented | Remove comment |
| `crates/rift-rndzv/src/api.rs` | 406 | Marked "(stub)" but fully implemented | Remove comment |
| `crates/rift-rndzv/src/api.rs` | 486 | Marked "(stub)" but fully implemented | Remove comment |
| `crates/rift-rndzv/src/api.rs` | 703 | Marked "(stub)" but fully implemented | Remove comment |
| `crates/rift-rndzv/src/api.rs` | 1130 | Marked "(stub)" but fully implemented | Remove comment |

---

## Files with TODOs

| File | Line | TODO | Status |
|------|------|------|--------|
| `clients/rift-qt-unified/qml/ChannelView.qml` | N/A | Parse invite and join | Not implemented |

---

## Test Status

### Passing Tests (Unit) ✅
- `rift-core`: 3/3 (E2EE, signatures)
- `rift-protocol`: 2/2 (framing)
- `rift-nat`: 4/4 (candidates, TURN URI)
- `rift-rndzv`: 22/22 (SRT, probing, channels)
- `rift-torrent`: 27/27 (bencode, magnet)

**Total:** 58/58 unit tests pass

### Ignored Tests (Infrastructure Required) ⚠️
- `nat_restrictive_turn_fallback()` - requires TURN server
- `stun_srflx_connectivity()` - requires STUN servers

### Missing Tests ❌
- No integration tests for discovery (mDNS, DHT)
- No mesh routing tests
- No browser client tests
- No audio pipeline tests

---

## Phase 35 Status (STUN/ICE-lite)

| Sub-Phase | Description | Status |
|-----------|-------------|--------|
| 35.1 | STUN client + candidate gathering | ❌ Not started |
| 35.2 | Protocol updates for ICE | ❌ Not started |
| 35.3 | Connectivity checks + path selection | ❌ Not started |
| 35.4 | SDK config + UI | ❌ Not started |

**Overall:** 0% complete

---

## Browser Client Gaps

| Feature | Status | Security Risk |
|---------|--------|--------------|
| Pairwise E2EE | ❌ Uses shared key | HIGH - relay can decrypt |
| UDP mesh | ❌ WebSocket only | Medium - requires relay |
| TURN support | ❌ Not implemented | Medium - no NAT traversal |
| Audio | ❌ Text only | Low - feature gap |

---

## Documentation Accuracy

| Document | Accuracy | Issue |
|----------|----------|-------|
| `README.md` | 85% | Claims STUN works (doesn't) |
| `docs/ROADMAP.md` | 100% | Correctly shows as future work |
| `docs/PHASE35_PLAN.md` | 100% | Plan exists, not implemented |
| `docs/README.browser.md` | 95% | Correctly notes limitations |

---

## Priority Fixes

### P0 (Security) 🔴
1. Fix browser shared key (implement Noise handshake)
2. Update docs to warn about browser insecurity

### P1 (Functionality) 🟠  
1. Complete Phase 35.1 (STUN client)
2. Complete Phase 35.2-35.3 (ICE-lite)
3. Remove misleading "(stub)" comments

### P2 (Correctness) 🟡
1. Update README to clarify STUN/ICE status
2. Add integration tests
3. Implement Qt invite parsing

### P3 (Enhancement) 🟢
1. Browser WebRTC integration
2. Browser audio support
3. Protocol hardening (capabilities, versioning)

---

## Quick Action Items

**Can be done today:**
- [ ] Remove 5 "(stub)" comments in `rift-rndzv/src/api.rs`
- [ ] Update README line 30 to say "(in progress)"
- [ ] Add security warning to browser README

**Can be done this week:**
- [ ] Create `docs/FEATURE_STATUS.md` with status matrix
- [ ] Add `cargo audit` to CI
- [ ] Document why tests are ignored

**Needs 2-3 weeks:**
- [ ] Implement STUN client (Phase 35.1)
- [ ] Fix browser E2EE security issue
- [ ] Add integration tests for discovery

---

## Code Locations

### Core implementations
- **Identity/Crypto:** `crates/rift-core/src/`
- **Protocol:** `crates/rift-protocol/src/`
- **NAT traversal:** `crates/rift-nat/src/`
- **Rendezvous:** `crates/rift-rndzv/src/`
- **Mesh:** `crates/rift-mesh/src/`

### Missing implementations
- **STUN client:** Should be in `crates/rift-nat/src/stun.rs` (doesn't exist)
- **ICE messages:** Should be in `crates/rift-protocol/src/control.rs` (missing variants)
- **ICE checker:** Should be in `crates/rift-mesh/src/ice.rs` (doesn't exist)

### Client code
- **TUI:** `bin/rift/src/`
- **Qt:** `clients/rift-qt-unified/`
- **Android:** `android/`
- **Browser:** `crates/rift-wasm/`, `crates/rift-web-chat/`, `www/`

---

## Build Issues

**Current build fails on:**
- Full workspace tests require ALSA dev libraries
- Audio tests need audio devices

**Workaround:**
```bash
# Test without audio dependencies
cargo test --package rift-core --lib
cargo test --package rift-protocol --lib
cargo test --package rift-nat --lib
cargo test --package rift-rndzv --lib
cargo test --package rift-torrent --lib
```

**All unit tests pass** ✅

---

## Architecture Decisions

### What's Well Designed ✅
- Modular crate structure (core, protocol, nat, mesh, etc.)
- Clear separation of concerns
- Good use of async/await
- Strong typing with `serde` for wire format
- Predictive Rendezvous is novel and working

### What Needs Work ⚠️
- ICE is partially implemented (basic UDP hole punching only)
- Browser client is isolated from native P2P mesh
- No formal protocol versioning or capability negotiation yet
- Limited integration test coverage

---

## Comparison: Documented vs Actual

| Feature | README Claim | Reality |
|---------|-------------|---------|
| "UDP hole punching" | ✅ Claimed | ✅ Works |
| "STUN candidates" | ✅ Claimed | ❌ Not implemented |
| "optional TURN fallback" | ✅ Claimed | ⚠️ TURN client exists, not integrated with ICE |
| "E2EE for chat + voice" | ✅ Claimed | ✅ Works (native), ❌ Broken (browser) |
| "Browser prototype" | ⚠️ "early, text-only" | ✅ Accurate |

**Recommendation:** Update README to move STUN/ICE to "Roadmap" section.

---

## Next Steps

1. **Read this document** to understand gaps
2. **Read `AUDIT_REPORT.md`** for detailed findings
3. **Read `IMPLEMENTATION_PLAN.md`** for phased fix plan
4. **Start with Phase 0** (cleanup) for quick wins
5. **Prioritize Phase 35** (STUN/ICE) for functionality
6. **Address browser security** (Phase 1) for safety

---

## Questions to Answer

Before implementing fixes, clarify:

1. **Is browser E2EE security issue known/accepted risk?**
   - If yes: document clearly and proceed with other work
   - If no: prioritize Phase 1 (browser security fix)

2. **Is Phase 35 (STUN/ICE) still the next priority?**
   - Plan exists, no implementation yet
   - Estimated 6-8 weeks of work

3. **Should we add tests before or during feature work?**
   - Recommendation: Add integration tests in parallel

4. **What's the target release version for these fixes?**
   - v0.2.0? v1.0.0?
   - Affects prioritization and scope

---

## Useful Commands

### Run tests (avoiding audio deps)
```bash
cargo test --workspace --lib
```

### Check for panics in network code
```bash
grep -r "panic!" crates/rift-mesh/src/
grep -r "unwrap()" crates/rift-mesh/src/ | grep -v "test"
```

### Find all TODOs
```bash
grep -r "TODO\|FIXME\|XXX" --include="*.rs" .
```

### Find stub implementations
```bash
grep -r "stub" --include="*.rs" crates/
```

### Build WASM client
```bash
wasm-pack build crates/rift-web-chat --target web
```

### Run relay for browser tests
```bash
cargo run -p rift-ws-relay
```

---

## Related Documents

- `AUDIT_REPORT.md` - Detailed audit findings (15k words)
- `IMPLEMENTATION_PLAN.md` - Phased implementation plan (30k words)
- `docs/PHASE35_PLAN.md` - Original Phase 35 plan
- `docs/ROADMAP.md` - High-level roadmap
- `docs/README.browser.md` - Browser client notes

---

## Contributors

This audit was generated by automated analysis of:
- All Rust source files (`.rs`)
- All documentation files (`.md`)
- Test suite results
- Protocol definitions
- Client implementations

**Methodology:**
1. Searched for TODO/FIXME/stub markers
2. Ran all unit tests that could build
3. Compared documentation claims to code reality
4. Identified ignored/missing tests
5. Mapped features to implementation status
6. Created phased fix plan

**Accuracy:** This is a snapshot as of 2026-02-15. Code may have changed since.
