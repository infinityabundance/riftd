# rndzv 2.0 — Phased Plan

rndzv 1.0 establishes Predictive Rendezvous as a functioning coordination and
network/session layer based on Semantic Rendezvous Tokens (SRTs), time-based
deterministic schedules, and optional infrastructure.

rndzv 2.0 focuses on stronger semantics, better composability, clearer
guarantees, and broader applicability while remaining backward-compatible with
rndzv 1.x.

## Phase 0 — Freeze rndzv 1.x contract (precondition)
- Declare rndzv 1.x public API stable.
- Version SRT format and commit to v1 support.
- Add compatibility tests to ensure v2 parses v1 SRTs unchanged.

## Phase 1 — Formalize the rendezvous model
- Define a formal rendezvous model and invariants.
- Validate SRTs against impossible or contradictory constraints.
- Publish a “Rendezvous Semantics” document.

## Phase 2 — SRTs as composable programs
- Introduce SRT components (time, identity, strategy, escalation).
- Allow safe composition and canonical normalization.

## Phase 3 — Multi-party and group rendezvous
- Define group rendezvous semantics and convergence rules.
- Extend sessions/channels for dynamic membership.

## Phase 4 — Predictive routing and multi-hop rndzv
- Define predictive forwarding and rendezvous-aware relays.
- Explore rendezvous chains when no direct path exists.

## Phase 5 — Security semantics as first-class
- Add security modes and time-bound identity binding.
- Add rendezvous-level abuse controls.

## Phase 6 — Naming beyond DNS
- Define self-certifying rndzv names mapped to SRTs by pure function.
- Allow optional alias layers (local address books, registries).

## Phase 7 — Observability and predictive feedback
- Extend metrics into local predictive hints.
- No ML dependency and no global telemetry.

## Phase 8 — Cross-domain applications
- Apply rndzv to device pairing, intermittent sync, agents, and robotics.
- Extract common coordination patterns.

## Phase 9 — Documentation, prior art, and preservation
- Publish a rndzv 2.0 architectural paper.
- Freeze historical documents and preserve terminology.

## Phase 10+ — Open, reserved
Future phases must be appended explicitly and justified by new constraints or
empirical findings.
