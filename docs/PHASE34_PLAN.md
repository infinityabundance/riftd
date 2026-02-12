# Phase 34 Plan: Reliability + E2EE Hardening

This plan focuses on the top-priority must-haves: STUN integration, basic E2EE,
and improved hole punching. It is intended as a concrete execution plan with
PR-sized slices.

## Goals
- Ship ICE-lite candidate exchange and path selection.
- Ship authenticated E2EE key exchange (X25519 + Ed25519).
- Improve hole-punching reliability with retries, keep-alives, and NAT type hints.

## Tasks (By Area)

### Protocol (rift-protocol)
- Add ICE messages: `IceCandidates`, `IceCheck`, `IceCheckAck`.
- Add key exchange messages: `KeyInit`, `KeyResp`.
- Add `EncryptedPayload` wrapper with `alg` field.
- Update versioning rules for V1/V2 coexistence.

### NAT (rift-nat)
- STUN binding requests to collect srflx candidates.
- NAT type heuristics (open vs natted vs symmetric-ish).
- Candidate gathering: host + srflx (+ relay later).
- Keep-alive timers per candidate pair.

### Mesh (rift-mesh)
- ICE-lite connectivity checks.
- Path scoring and selection (direct > srflx > relay > TURN).
- Trigger key exchange once path stable.
- Encrypt chat/voice using negotiated session keys.

### Core (rift-core)
- X25519 ephemeral keypair support.
- Ed25519 signing for ephemeral key authentication.
- HKDF session key derivation.

### Media (rift-media)
- Encrypt Opus payloads using session keys (SRTP-like envelope).
- Nonce derivation based on seq/timestamp.

## PR Breakdown

1. **Protocol: ICE + Key Exchange + E2EE wrapper**
   - Extend `ControlMessage` and `RiftPayload`.
   - Update PROTOCOL.md spec.

2. **NAT: STUN + candidate gathering**
   - Add STUN client + srflx candidate generation.
   - Expose config via SDK.

3. **Mesh: ICE-lite connectivity checks**
   - Candidate exchange + connectivity probes.
   - Path scoring and route updates.

4. **Core: X25519 + Ed25519 signing**
   - New key derivation helpers.
   - Session key HKDF.

5. **Mesh: E2EE key exchange**
   - Initiate KeyInit/KeyResp.
   - Key storage per peer/session.

6. **Media: E2EE payload encryption**
   - Encrypt Opus packets with session key.
   - Decrypt on receive.

7. **Docs + tests**
   - Update PROTOCOL.md and SECURITY.md.
   - Add basic E2EE test harness.

## Deliverables
- Protocol V2 with optional ICE + E2EE.
- ICE-lite connectivity selection in mesh.
- End-to-end encryption for chat + voice.
- Improved NAT traversal reliability.
