# Predictive Rendezvous — Design Rationale

## Why PR Exists
Predictive Rendezvous (PR) exists to enable serverless, deterministic coordination between peers who already share intent. It minimizes reliance on centralized infrastructure while remaining compatible with existing transports and discovery mechanisms.

## What PR Does Not Solve
- It does **not** guarantee reachability without any candidate address knowledge.
- It does **not** replace encryption, authentication, or authorization.
- It does **not** remove all infrastructure needs in large-scale discovery problems.

## Core Assumptions
- Peers share a seed and a time window (SRT).
- Peers can derive the same slot schedule with bounded clock skew.
- Peers have at least one candidate address path during the window.

## Scope Boundaries
PR is a coordination layer. It is intentionally minimal and does not define transport semantics or application policy.
