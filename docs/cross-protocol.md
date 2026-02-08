# Predictive Rendezvous Across Protocols (Protocol-Agnostic View)

This document demonstrates that Predictive Rendezvous (PR) is a coordination model, not a protocol. The same structure applies across multiple domains.

## Common Structure
- **Shared intent**: a compact identifier that both peers already agree on.
- **Time window**: a bounded interval for coordinated probing.
- **Deterministic schedule**: a shared function that maps time slots to probing behavior.

These invariants remain the same regardless of transport or application domain.

## Real-Time Media
**Typical coordination today:** signaling servers, ICE, STUN/TURN, or rendezvous services.

**PR reframing:** peers share an SRT (seed + window). They deterministically probe candidate addresses during the window. This can complement or reduce reliance on signaling infrastructure, but does not replace media transport or encryption.

## File Distribution
**Typical coordination today:** trackers, DHT, peer exchange.

**PR reframing:** peers with the same content identifier can derive a PR seed and attempt deterministic rendezvous windows. PR does not replace DHT/trackers; it provides a bounded coordination path that can bootstrap or verify candidate peers.

## Device Pairing
**Typical coordination today:** QR codes, short codes, BLE pairing, local discovery.

**PR reframing:** the pairing token encodes an SRT. Both devices use a time window and deterministic schedule to discover each other without a central broker, using local candidate addresses (LAN, BLE, or Wi‑Fi Direct).

## Agent Coordination
**Typical coordination today:** directories, task brokers, rendezvous servers.

**PR reframing:** agents share a task/intent identifier and a time window. They use deterministic schedules to attempt direct contact. Infrastructure can remain optional for broader discovery.

## Notes
This document emphasizes invariants and domain mappings, not implementation details. PR remains a coordination abstraction that can sit above different transports and below different applications.
