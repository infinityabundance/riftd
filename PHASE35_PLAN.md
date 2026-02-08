# Phase 35 Plan: STUN + ICE-lite Reliability

Goal: Improve public-internet reachability by gathering public candidates,
running ICE-lite connectivity checks, and selecting the best path.

## Scope
- STUN client for public (srflx) candidate discovery.
- ICE-lite candidate exchange and connectivity checks.
- Candidate scoring and path selection (direct > punched > relayed > TURN).
- Keep-alives and retry logic for NAT traversal reliability.

## Deliverables
1. **rift-nat**
   - STUN client (configurable servers).
   - NAT type detection (best-effort).
   - Candidate gathering (host + srflx).
   - Keep-alive timer per candidate.

2. **rift-protocol**
   - Confirm `Hello` includes candidates and supported versions.
   - `IceCandidates`, `IceCheck`, `IceCheckAck` in ControlMessage.

3. **rift-mesh**
   - Connectivity checker (ICE-lite).
   - Candidate scoring and active path selection.
   - Path switching and downgrade to relay if direct fails.

4. **rift-sdk + clients**
   - Config for STUN servers and ICE-lite enable toggle.
   - Surface link stats for path decisions.

## Rollout Phases
1. **Phase 35.1**: STUN client + candidate gathering in `rift-nat`.
2. **Phase 35.2**: Protocol updates + candidate exchange in `rift-mesh`.
3. **Phase 35.3**: ICE-lite checks + path selection + keep-alives.
4. **Phase 35.4**: SDK/config exposure + client status UI.

## Testing
- Local LAN test (host candidates only).
- WAN test with srflx candidates via STUN.
- Retry/keep-alive validation (simulated packet loss).

