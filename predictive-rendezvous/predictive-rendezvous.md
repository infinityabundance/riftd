# Predictive Rendezvous

**Predictive Rendezvous (PR)** is a coordination architecture for establishing peer-to-peer connections using **shared foreknowledge** rather than live negotiation or infrastructure-mediated discovery.

This document provides a **technical and conceptual overview** of Predictive Rendezvous as used in riftd. It explains the underlying model, terminology, and design intent independently of any specific implementation.

---

## Overview

Traditional peer-to-peer systems treat connection establishment as a discovery problem: peers must learn each other’s current network locations (IP addresses, ports, candidates) in order to connect. This framing leads naturally to signaling servers, STUN/TURN, trackers, and DHTs.

Predictive Rendezvous adopts a different model:

> **Peers do not discover each other — they predict each other.**

If peers share:
- intent,
- a time context, and
- sufficient shared entropy,

then they can independently execute the **same deterministic rendezvous plan**, causing their network states to converge without exchanging runtime state or relying on third-party infrastructure.

Infrastructure becomes optional rather than fundamental.

---

## Position in riftd

Within riftd, Predictive Rendezvous is a **connection-establishment strategy**, not a transport or media protocol.

Conceptually:

Predictive Rendezvous
↓
UDP path established
↓
riftd session + encryption + media


PR operates **above transport** and **below session/media**, supplying deterministic parameters (timing, ports, behavior) that allow peers to establish a reachable path. Once a path exists, riftd’s existing session and media layers take over.

When foreknowledge exists, Predictive Rendezvous is intended to be the **primary** rendezvous mechanism. Legacy NAT traversal and relays remain available as secondary options.

---

## Core Concepts

### Semantic Rendezvous Token (SRT)

A **Semantic Rendezvous Token (SRT)** encapsulates the shared foreknowledge required for Predictive Rendezvous.

An SRT is **not**:
- an address,
- a locator,
- a room ID.

It is an **executable rendezvous plan**.

An SRT may encode:
- shared entropy (e.g. a cryptographic seed),
- identity constraints (which peers are permitted),
- a time model (start time, window, slot duration),
- a class of deterministic behavior strategies,
- escalation policies.

**Crucially, an SRT contains no network addresses.**

SRTs are exchanged **out-of-band** (chat messages, email, QR codes, documents, physical transfer) and reconstructed independently by all participants.

---

### Time as an Addressing Dimension

Predictive Rendezvous treats **time** as a first-class addressing dimension.

Rather than resolving a peer to a static `(IP, port)` tuple, peers compute a **time-indexed behavior function**:



A(t) = f(seed, t, role)


At any given time slot, this function determines:
- which local resources to use (e.g. port offsets),
- which remote behavior to expect,
- when and how traffic should be emitted.

Addresses are no longer static locations — they are **trajectories** through a shared plan.

---

### Deterministic Slot Schedules

Time is divided into fixed-size slots within a rendezvous window.

For a given slot index, peers compute deterministic parameters using a pseudorandom function seeded by:
- the SRT entropy,
- the slot index,
- an optional role tag (e.g. caller / callee).

Derived parameters may include:
- local port offsets,
- expected remote port offsets,
- burst patterns and timing jitter,
- escalation flags.

Because all peers execute the same computation, **compatible schedules emerge without communication**.

---

### Convergence Without Negotiation

NATs, firewalls, and operating systems evolve state in response to outbound traffic according to deterministic rules.

By emitting traffic according to a shared schedule:
- peers force predictable state transitions,
- mappings evolve in synchronized ways,
- rendezvous occurs when configurations coincide during a slot.

No live exchange of candidates or state is required.  
Convergence arises from **deterministic co-execution**, not negotiation.

---

## Why Predictive Rendezvous Exists

Most infrastructure in P2P systems exists to synchronize **transient state**:
- NAT mappings,
- socket bindings,
- ephemeral reachability information.

Predictive Rendezvous internalizes this synchronization by replacing it with:
- shared foreknowledge,
- deterministic schedules,
- time-bounded intent.

This reframing:
- reduces dependency on infrastructure,
- makes failure modes explicit,
- aligns coordination with how humans already rendezvous (“meet here at this time”).

---

## Example Flow (Conceptual)

1. A peer generates an SRT describing a future rendezvous.
2. The SRT is shared out-of-band.
3. At the agreed time window, peers activate a rendezvous runner.
4. Each peer independently computes slot schedules.
5. Probes are emitted according to the schedule.
6. When convergence occurs, a direct path is established.
7. riftd upgrades the path to a secure session and media flow.

No signaling servers are required for this process.

---

## Limitations

Predictive Rendezvous is intentionally scoped.

It does **not** guarantee connectivity in all environments. It applies when:
- peers share foreknowledge,
- peers share a time reference,
- deterministic behavior exists at the relevant abstraction level.

If no prior context exists and no information can be exchanged, no coordination mechanism is possible. These limits are fundamental, not implementation defects.

---

## Relationship to the Prior Art Paper

This document is a **repository-local conceptual guide**.

The formal architectural description and prior-art framing are captured in the paper:

> **Predictive Rendezvous: Time–Intent–Deterministic Peer Coordination Without Infrastructure**  
> Riaan de Beer, 2026
> **[https://doi.org/10.5281/zenodo.18528430]**


The paper establishes terminology, scope, and generality.  
This README contextualizes those ideas within riftd.

---

## Implementation Notes

This document intentionally avoids prescribing:
- exact wire formats,
- specific cryptographic primitives,
- concrete APIs.

Refer to the Rust crate(s) under this directory for:
- SRT encoding/decoding,
- deterministic schedule functions,
- rendezvous state machines,
- test scaffolding.

Terminology and structure here should remain aligned with code for clarity and maintainability.

---

## Summary

Predictive Rendezvous introduces a missing coordination layer between transport and application semantics:

- **coordination by prediction, not discovery**
- **time and intent as protocol primitives**
- **infrastructure optional, not assumed**

This document describes the conceptual foundation on which riftd’s Predictive Rendezvous mechanisms are built.
