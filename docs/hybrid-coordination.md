# Hybrid Coordination Models (Exploratory)

This document describes optional hybrid coordination patterns that combine Predictive Rendezvous (PR) with infrastructure-assisted mechanisms (DHTs, relays, directories). These are architectural patterns, not prescriptions, and are **not part of the core PR protocol**.

## Hybrid Modes
- **PR-first, infra-assisted fallback**: attempt PR first, fall back to DHT/relay discovery if no convergence within the rendezvous window.
- **Infra-first, PR-verified**: use infrastructure to discover candidates, then use PR to verify or accelerate convergence.
- **Parallel execution**: run PR and infrastructure lookup simultaneously; accept the earliest successful path.

## Where Hybridization Helps
- **Cold start**: no recent addresses or shared network context.
- **Large, long-lived swarms**: PR can provide deterministic short windows while DHTs provide broad discovery.
- **Partial or unreliable infrastructure**: PR offers a predictable path when infra is slow or degraded.

## Trade-offs and Boundaries
- Hybrid approaches improve reachability but increase complexity.
- Infrastructure may leak metadata or create dependency; PR reduces reliance but cannot replace discovery when no candidate addresses exist.
- PR remains the coordination abstraction; infrastructure remains optional and non-fundamental.

## Scope Notes
All hybrid patterns are **exploratory** and **optional**. They should be treated as guidance for experimentation, not as changes to the core protocol.
