# Hybrid Rendezvous Modes

This document describes optional hybrid coordination modes that combine
Predictive Rendezvous (`rndzv`) with infrastructure hints or relay fallback.
The core invariants remain unchanged: pure rndzv is always valid, and any
infrastructure is strictly optional.

## Modes

### PureRndzv (default)
Uses only the SRT and deterministic schedule. No hints, no relays.

### RndzvThenRelay
Attempt pure rndzv within a time budget. If it times out, fall back to a relay
path. Metrics mark `fallback_used = true` with `fallback_method = "relay"`.

### ParallelRndzvAndRelay
Run rndzv and relay attempts concurrently and accept the first success. This is
useful when cold starts or high-latency paths are expected.

### RndzvWithDhtHints
Query a DHT (or similar directory) for candidate addresses and seed the initial
probe targets with those hints. The deterministic rndzv schedule remains the
canonical mechanism.

## Trade-offs
- Relays improve reachability but cost infrastructure and latency.
- DHT hints can accelerate convergence but should never be required.
- Pure rndzv remains the recommended default for low-friction coordination.

## Invariants
- No mode changes the SRT format.
- No mode requires infrastructure for correctness.
- Hybrid modes are optimizations, not dependencies.
