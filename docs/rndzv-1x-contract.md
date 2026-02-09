# rndzv 1.x Contract (Stable)

This document freezes the rndzv 1.x public contract so rndzv 2.0 can evolve
without destabilizing existing applications.

## Stable API Surface

### SRT (Semantic Rendezvous Token)
- `SemanticRendezvousToken` fields:
  - `space: RendezvousSpaceId`
  - `seed: [u8; 32]`
  - `identities: IdentityConstraints`
  - `time_model: TimeModel`
  - `search_strategy: SearchStrategy`
  - `escalation: EscalationPolicy`
- SRT URI encoding:
  - Scheme: `riftd-srt://`
  - Version: `v1` (required)
  - Query fields: `space`, `seed`, `t0`, `tw`, `slot`, `ss`, `esc`, optional `ids`
- `SemanticRendezvousToken::to_uri` / `from_uri` remain stable for v1.

### Rendezvous Interfaces
- `RndzvConnector` and `RndzvListener`:
  - `RndzvConnector::connect(target)` returns `RndzvOutcome`
  - `RndzvListener::accept()` returns `RndzvOutcome`
- `RndzvSession`:
  - session identity and peer ids
  - `open_channel(ChannelKind)` for `RndzvChannel`
  - `shutdown()` for clean teardown
- `RndzvChannel`:
  - `send(&[u8])` and `recv()` for message exchange

### Scheduling
- Deterministic slot schedule driven by:
  - `TimeModel` (`t0`, `window_secs`, `slot_ms`)
  - PRF over `seed || role || slot_index`
- `compute_slot_params(...)` behavior remains stable.

## Versioning
- SRT `v1` is supported indefinitely.
- Future versions must be additive or negotiated explicitly.

## Experimental / Unstable
These are explicitly *not* guaranteed stable in 1.x:
- Hybrid modes (`HybridMode`, DHT hints, relay fallback paths).
- Relay / DHT integration behavior and heuristics.
- Reliable ordered channel semantics (protocol details may evolve).
- Crypto handshake details and frame formats.

## Compatibility Tests
The test suite must include v1 parsing tests to ensure any v2 changes continue
to accept v1 SRTs unchanged.
