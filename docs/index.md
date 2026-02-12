# Predictive Rendezvous Documentation Index

## Core Concepts
- [predictive-rendezvous.md](predictive-rendezvous.md) — architecture and SRT format
- [formalization.md](formalization.md) — minimal formal model
- [design-rationale.md](design-rationale.md) — why PR exists and boundaries
- [pr-security.md](pr-security.md) — threat analysis (coordination layer)

## Protocol & Code
- [PROTOCOL.md](PROTOCOL.md) — wire protocol and message types
- [CODE.md](CODE.md) — high-level code map
- [SECURITY.md](SECURITY.md) — threat model and security checklist

## Crate Documentation
- [rift-torrent.md](rift-torrent.md) — SRT-based torrent peer discovery

## Papers
- [papers/predictive_rendezvous_prior_art.md](papers/predictive_rendezvous_prior_art.md) — prior‑art and v2 additions

## Tooling
- [srt-tooling.md](srt-tooling.md) — SRT generation/inspection
- [srt-invites.md](srt-invites.md) — SRT invite UX and sharing

## Guides
- [README.browser.md](README.browser.md) — browser WASM prototype
- [README.android.md](README.android.md) — Android build guide
- [TURN_GUIDE.md](TURN_GUIDE.md) — self-hosted TURN setup

## Experiments
- [experiments/predictive-torrents.md](experiments/predictive-torrents.md) — PTR experiment notes
- [simulations/predictive-rendezvous.md](simulations/predictive-rendezvous.md) — NAT/topology simulation harness

## Cross‑Domain
- [cross-protocol.md](cross-protocol.md) — protocol‑agnostic mapping
- [hybrid-coordination.md](hybrid-coordination.md) — optional hybrid patterns
- [hybrid-modes.md](hybrid-modes.md) — hybrid modes for rndzv + relay/DHT hints
- [rndzv-1x-contract.md](rndzv-1x-contract.md) — stable rndzv 1.x public contract
- [rndzv-2.0-plan.md](rndzv-2.0-plan.md) — rndzv 2.0 phased plan
- [future-directions.md](future-directions.md) — synthesis and future work

## Project Management
- [CHANGELOG.md](CHANGELOG.md) — release history
- [ROADMAP.md](ROADMAP.md) — planned next steps
- [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) — release process
- [PHASE34_PLAN.md](PHASE34_PLAN.md) — ICE/E2EE reliability work
- [PHASE35_PLAN.md](PHASE35_PLAN.md) — phase 35 plan
- [OPTIMIZATION_REPORT.md](OPTIMIZATION_REPORT.md) — performance notes

## Status
- Core phases (SRT, scheduling, runner, metrics): complete
- Experiments (PTR, simulations): exploratory
- Hybrid and future directions: exploratory
