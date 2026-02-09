# Predictive Rendezvous — Minimal Formalization

This document provides a lightweight formal model for Predictive Rendezvous (PR). It is intended to clarify assumptions and failure modes without binding to specific cryptographic primitives or proving full correctness.

## Minimal Abstract Model
**Participants:** two peers `A` and `B`.

**Shared state:**
- `seed` (shared deterministic seed)
- `t0` (start time)
- `window` (duration)
- `slot_ms` (slot duration)
- `role` (caller/callee/symmetric)

**Deterministic transition function:**
A function `F(seed, role, slot_index)` produces per-slot parameters:
- `local_port_offset`
- `remote_port_offset`
- `burst_pattern`

**Slot index:**
```
slot_index(t) = floor((t - t0) / slot_ms)
```
for any `t` in `[t0, t0 + window)`.

**Convergence condition:**
Rendezvous succeeds if, within the same active slot, both peers emit probes such that at least one probe from each peer reaches the other’s candidate address set and is accepted by identity constraints.

## Pseudocode Model
```
for t in [t0, t0 + window):
    n = slot_index(t)
    params = F(seed, role, n)
    emit probes(params) to candidate addresses
    if valid probe received from peer during slot n:
        rendezvous succeeds
```

## Assumptions
- Both peers share the same `seed` and time window.
- Both peers can derive the same slot index (clock skew bounded by slot duration).
- Each peer has at least one candidate address that is reachable during the window.
- Identity constraints are satisfiable by the intended peer.

## Necessary and Sufficient Conditions
**Necessary:**
- Shared `seed` and compatible time window.
- At least one reachable candidate address path in the window.
- Slot schedules overlap (bounded clock skew).

**Sufficient (informal):**
- If both peers compute identical slot indices for some window interval and each emits probes to an address that reaches the other, then rendezvous succeeds.

## Failure Cases
- **No shared intent:** different seeds or roles yield disjoint schedules.
- **Clock skew beyond slot duration:** peers compute different slot indices.
- **No candidate reachability:** no path exists between peers during the window.
- **Unsatisfiable identity constraints:** probes are rejected even if delivered.

## Scope Notes
This formalization is intentionally minimal. It clarifies the logical structure of PR without asserting cryptographic security or probabilistic guarantees.
