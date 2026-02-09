Title: Predictive Rendezvous Swarm Experiments (Experimental)

Overview
- This note sketches a proof-of-concept for using Predictive Rendezvous (rndzv)
  as a bootstrap mechanism for swarm-style discovery.
- The goal is not a torrent client; it is a minimal experiment to see whether
  shared intent can yield initial peer contact without trackers or DHTs.

Setup
- All peers share a `ContentId` (infohash-like identifier).
- Each peer derives a shared SRT from the content id.
- A swarm-specific seed is derived per peer to diversify schedules.
- Peers that land in the same rendezvous bucket during a slot are treated as
  having "connected" for the experiment.

Implementation
- The harness lives in `crates/rndzv-sim/src/swarm.rs`.
- The core entrypoint is `simulate_swarm(content, peer_ids, ticks, buckets)`.
- Metrics captured:
  - connection graph (set of connected pairs),
  - degree per peer,
  - average degree.

Observations
- The simulation provides a coarse measure of how often peers "collide"
  in the same rendezvous bucket.
- It does not model real packet delivery, NAT binding quirks, or socket reuse.

Limitations
- This is a purely synthetic model, not a networking experiment.
- It does not implement piece exchange, tit-for-tat, or swarm maintenance.
- Results are qualitative and should not be used to claim real-world guarantees.

Next Steps (Optional)
- Add a richer simulator that feeds rendezvous outcomes back into the peer set.
- Introduce simple NAT models to stress the rendezvous collisions.
- Compare different slot sizes and bucket counts for density tradeoffs.
