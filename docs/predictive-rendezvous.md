# Predictive Rendezvous in riftd

## Overview
Predictive Rendezvous (PR) is a serverless rendezvous mechanism. Two peers share the same seed, time window, and intent, then independently run a deterministic schedule that tells them when and how to probe for each other. No coordination service is required; both sides derive the same slot sequence from the same inputs.

## Semantic Rendezvous Token (SRT)
An SRT is not an IP address and not a room ID. It is an executable rendezvous plan that fully describes how two peers should derive the same schedule.

Core fields (current implementation):
- `seed`: 32-byte value that deterministically drives the schedule.
- `time_model`: `{ t0, window_secs, slot_ms }`.
- `identities`: allowed peer fingerprints.
- `search_strategy`: deterministic strategy selection.
- `escalation`: escalation policy selection.

Example SRT URI:
```
riftd-srt://v1?seed=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&t0=1700000000&tw=120&slot=250&ss=basic&esc=none&ids=0101010101010101010101010101010101010101010101010101010101010101
```

Decoded form (conceptual):
```
seed = [0u8; 32]
time_model = { t0: 1700000000, window_secs: 120, slot_ms: 250 }
identities.allowed_fingerprints = [0x01..01 (32 bytes)]
search_strategy = BasicDeterministic
escalation = None
```

## Time Model and Slots
The time model defines the rendezvous window and the slot duration:
- `t0`: anchor time in Unix seconds.
- `window_secs`: total rendezvous window size.
- `slot_ms`: duration of a slot in milliseconds.

Slot index is computed as:
```
slot_index = floor((now_ms - t0_ms) / slot_ms)
```
If `now_ms` is before `t0` or after the window ends, there is no active slot.

## Deterministic Schedule
The core schedule function is `compute_slot_params`. It derives per-slot parameters using a strong PRF over `seed || role_tag || slot_index`:
- `local_port_offset`
- `remote_port_offset`
- `burst_pattern`

Role-based asymmetry (`Caller`, `Callee`, or `Symmetric`) ensures both peers can derive either mirrored or identical sequences as required.

## RendezvousRunner
`RendezvousRunner` is the high-level state machine that uses the SRT and role to emit probes for the current slot. It is designed to be wired into riftd's networking stack later.

- Time is abstracted via `Clock` (`now_unix_ms`).
- Networking is abstracted via `UdpIo` (`send_probe`).

This makes the runner testable without real sockets while keeping the interface ready for integration.

## No-IP and No-Server Design
SRTs do not encode IPs or ports. The schedule is derived only from seed, time model, and policy metadata.

Peers can use prior knowledge (historical addresses, local discovery, or user-provided hints) to supply candidate remote addresses, but the SRT itself remains a pure rendezvous plan with zero infrastructure requirements.

## Using PR from the CLI
The `rift` CLI provides helper commands for generating and inspecting SRTs. These commands do not perform any networking; they only create or decode tokens.

Generate an invite targeted at a peer fingerprint (32-byte hex):
```
rift tir-invite <64-hex-chars>
```

Decode and validate an invite:
```
rift tir-accept <riftd-srt://...>
```

When `tir-accept` runs, it checks the SRT's identity constraints against the local identity if available and prints a decoded summary.

## Metrics and Logging
Predictive Rendezvous emits debug-level `tracing` logs with the rendezvous ID and slot metadata. These logs include per-slot emission, success, and timeout summaries.

The async runner also collects a `RendezvousMetrics` snapshot, including:
- `slots_attempted` and `slots_succeeded`
- `probes_sent` and `probes_received`
- `total_duration_ms` and `time_to_first_packet_ms`
- `nat_behavior_notes` (e.g. `port_preserving`, `high_variance`)

Use `rift pr-test <peer-addr> --show-metrics` to print a summarized metrics table for manual inspection.

## Prior-Art Paper
See `docs/papers/predictive_rendezvous_prior_art.md` for the prior-art draft. A v2 revision adds diagrams and a proof-of-concept section; PDF generation and external publication are pending.
