# Rift Security Checklist and Threat Model

This document captures a lightweight security checklist and threat model for Rift.
It is intended to guide audits and prevent regressions as the protocol evolves.

## Threat Model (High Level)

### Assets
- **Confidentiality** of chat and voice content.
- **Integrity** of messages (no tampering or impersonation).
- **Availability** of peer connections and voice sessions.
- **Identity binding** between `PeerId` and public key.

### Adversaries
- **On-path attacker** (can observe, replay, or tamper with packets).
- **Off-path attacker** (can spoof, inject packets, or DoS).
- **Malicious peer** (joins the channel and attempts to impersonate or disrupt).
- **Relay operator** (honest-but-curious or malicious relay peer).

### Trust Assumptions
- No central authority; TOFU + optional shared secret for access.
- Noise provides transport-level authenticated encryption.
- Pairwise E2EE provides end-to-end confidentiality for chat/voice.

### Limitations (Current)
- No post-compromise security (PCS). Compromise of a live endpoint can reveal session keys.
- No deniability or metadata protection beyond transport encryption.
- Group calls still rely on pairwise keys per peer (no MLS yet).

### Out of Scope (for now)
- Full PKI or federated identity.
- Formal verification of the protocol.
- Dedicated TURN infrastructure run by Rift.

## Threat Model (STRIDE Summary)

| Category | Example Threat | Current Mitigation |
| --- | --- | --- |
| **Spoofing** | Peer impersonation by forging `PeerId` | TOFU + known_hosts, Ed25519 identity checks |
| **Tampering** | Packet modification in transit | Noise transport AEAD + E2EE payload AEAD |
| **Repudiation** | Sender denies having sent a message | Limited: no non-repudiation; logs are local |
| **Information Disclosure** | Relay or on-path observer reads chat/voice | E2EE (AES-GCM) for chat/voice; relays see only metadata |
| **Denial of Service** | Flooding packets or handshake storms | Input validation, per-addr rate limiting, early drops |
| **Elevation of Privilege** | Unauthorized channel access | Optional channel shared secret (`Auth` token) |

## Security Checklist

### Identity / Key Management
- [ ] `PeerId` is derived from public key; mismatches are detected and logged.
- [ ] TOFU writes new peer keys to `known_hosts`.
- [ ] Key mismatch triggers warning and optional disconnect.
- [ ] Key rotation archives old keys for verification.

### Transport Security (Noise)
- [ ] Noise pattern and cipher suite are explicit constants.
- [ ] Ephemeral keys are generated from secure RNG.
- [ ] Session keys are not reused across reconnects.
- [ ] Optional periodic rekeying is enabled for long sessions.

### End-to-End Encryption
- [ ] E2EE wraps chat and voice payloads (not control/relay headers).
- [ ] Pairwise E2EE uses per-session X25519 with Ed25519-signed keys.
- [ ] Payloads are encrypted with AES-256-GCM and authenticated AAD.
- [ ] AAD binds ciphertext to header (seq/timestamp/source/session).
- [ ] Decrypt failures are logged and do not crash the node.

### NAT / Relay
- [ ] Relayed payloads keep destination metadata in clear.
- [ ] Inner payloads remain encrypted end-to-end.
- [ ] Relay selection does not expose session keys.
- [ ] TURN relays only see encrypted payloads but can observe metadata; prefer self-hosted TURN when possible.

### Auth / Access Control
- [ ] Optional channel shared secret enforced by `Auth` control message.
- [ ] Peers that fail auth are rejected early.

### DHT / Discovery
- [ ] DHT announcements do not leak plaintext channel secrets.
- [ ] Peer discovery does not implicitly trust reported identities.

### Logging / Auditability
- [ ] Security events are logged to audit log if configured.
- [ ] Logs do not contain plaintext chat/voice payloads.

### DoS / Abuse
- [ ] Rate-limit repeated handshake attempts per peer.
- [ ] Drop malformed or oversized frames early to avoid CPU exhaustion.
- [ ] Per-addr packet rate limiting is enabled with audit logging.

### Fuzzing / Parser Safety
- [ ] Protocol decoding is fuzzed (cargo-fuzz or AFL) against malformed frames.
- [ ] Decoder rejects oversized frames and invalid lengths deterministically.

## Phase 43 Hardening Notes
- Added max frame length enforcement in `rift-protocol` to reject oversized frames.
- Added per-address packet rate limiting in `rift-mesh` with security logging.
- Ensured secure randomness usage via `OsRng` across identity/ephemeral keys.

## Responsible Disclosure

If you discover a security issue, please report it privately:

- Email: `security@rift.dev` (replace with a real address before release)
- Include: version/commit, reproduction steps, and impact assessment.

We will acknowledge reports within 72 hours and coordinate a fix timeline.

## Suggested Tests

### Unit / Integration
- [ ] E2EE roundtrip with AAD mismatch failure.
- [ ] TOFU persists known_hosts and rejects mismatched keys.
- [ ] Relay path preserves E2EE secrecy (relay sees only envelopes).

### Manual
- [ ] Capture UDP traffic with a sniffer; confirm chat/voice are encrypted.
- [ ] Force key mismatch and confirm warning + optional disconnect.
- [ ] Verify rekey happens on interval and does not drop sessions.

## Review Cadence
- **Every protocol change**: re-run checklist.
- **Before release**: perform manual capture test and key mismatch test.
