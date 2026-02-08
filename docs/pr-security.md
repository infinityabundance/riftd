# Predictive Rendezvous — Security, Abuse, and Adversarial Considerations

This document analyzes Predictive Rendezvous (PR) under adversarial conditions. It focuses on coordination-layer risks and mitigations without claiming full security solutions.

## Threat Categories
- **Malicious peers**: adversaries obtain an SRT and attempt to impersonate or flood a rendezvous session.
- **Timing disruption**: adversaries intentionally skew timing or attempt to desynchronize slot schedules.
- **Probe flooding / amplification**: attackers exploit predictable schedules to trigger bursts or waste bandwidth.
- **Replay of SRTs**: old SRTs are replayed outside their intended windows.

## Architectural Mitigations (Coordination Layer)
- **Time-bounded validity**: short windows limit replay and stale token use.
- **Identity constraints**: SRTs may restrict acceptable fingerprints to reduce impersonation and noise.
- **Rate limiting**: rendezvous probing should enforce per-peer and per-window limits.

These mitigations improve robustness but are not complete security guarantees.

## Layer Separation
- **Coordination layer**: schedule derivation, slot timing, token scope.
- **Transport security**: encryption, integrity, and peer authentication are outside PR’s scope.
- **Application authorization**: access control and policy enforcement must live above PR.

## Notes
This is a threat analysis note, not a full security protocol. It outlines risks and partial mitigations without implying comprehensive protection.
