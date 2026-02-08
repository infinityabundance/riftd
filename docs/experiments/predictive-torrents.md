# Predictive Torrent Rendezvous (PTR) — Experimental Notes

## Overview
Predictive Torrent Rendezvous (PTR) is an experimental application of Predictive Rendezvous (PR) to swarm bootstrapping. The shared intent is a content identifier (e.g., a torrent infohash). Peers derive a shared SRT seed from that identifier and attempt to rendezvous without trackers or DHT bootstrap.

This is a proof-of-concept exploration only. It does not replace classic discovery mechanisms.

## Minimal Concept
- **Shared intent**: `infohash` (or equivalent content identifier).
- **Seed derivation**: `seed = HKDF(infohash || optional_salt)`.
- **Time model**: coarse-grained slots and longer windows (e.g., slot 500–2000 ms, window 60–300s).

## Experimental Setup
- Two or more peers start with the same content identifier.
- No trackers, no DHT bootstrap.
- Each peer runs a PR-style schedule that emits probes to a candidate address set.

## Proof-of-Concept Tool
A minimal CLI tool (`rift ptr-test`) derives a rendezvous seed from a content hash and attempts PR-style rendezvous to a target peer address. Example usage:
```
rift ptr-test 203.0.113.10:9000 --infohash <hex> --window-secs 60 --slot-ms 500 --show-metrics
```

It logs:
- time to first peer contact
- slots attempted
- whether fallback discovery was required

## Observations (Qualitative)
- **LAN / simple NAT**: rendezvous succeeds quickly when peers know each other's recent addresses.
- **No prior address knowledge**: rendezvous cannot converge without candidate targets.
- **Highly variable NAT**: rendezvous may stall unless slot size is increased or additional hints are supplied.

## Failure Conditions
- Disconnected networks or strict firewalls.
- No shared or historical address hints.
- Symmetric NAT behavior with high port variance.

## Notes
PTR is an experiment. It should be treated as a possible bootstrap assist, not a replacement for trackers/DHT.
