# Predictive Rendezvous — Synthesis and Future Directions

## Summary
Predictive Rendezvous (PR) works best when peers share intent and have at least some prior knowledge of each other’s candidate addresses (recent addresses, local discovery, or explicit hints). In controlled settings and simple NAT environments, deterministic slot schedules converge quickly and predictably. PR struggles when there is no shared address context or when network conditions are highly variable (e.g., strict symmetric NAT or hard firewall barriers).

The systems that benefit most from PR are those that already exchange out-of-band hints (chat, QR, short links) and can tolerate a brief coordinated probing window without requiring centralized infrastructure.

## Boundaries (Restated)
- **PR is a coordination layer**, not a transport protocol.
- **PR does not eliminate infrastructure universally**; it reduces dependency in cases where peers can pre-share intent and hints.
- **PR depends on shared foreknowledge** such as a shared seed and at least one candidate address path.

## Promising Extensions (Exploratory)
- **Hybrid PR + DHT** (experimental): use PR for short-lived, deterministic rendezvous windows while DHT or trackers provide broader address discovery.
- **Adaptive slot scheduling** (exploratory): adjust slot duration or burst patterns based on observed failures and NAT behavior.
- **Identity integration** (exploratory): bind SRTs to identity systems or signatures for stronger validation and abuse resistance.

## Scope Notes
All items in this document are experimental or exploratory and **not part of the core protocol**. They should be treated as follow-up work rather than changes to the original prior-art record.

## Follow-up Documents
Any new papers should be published as additive, dated follow-ups. The original prior-art paper remains intact and unchanged.
