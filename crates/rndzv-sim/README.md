# rndzv-sim

`rndzv-sim` is an in-memory simulation harness for Predictive Rendezvous. It
models peers, NAT mappings, and message delivery without real sockets.

## What it does
- Models peers with local/private addresses.
- Supports pluggable NAT behaviors.
- Runs discrete-time ticks to simulate rendezvous probes.

## Running scenarios
Add or update tests in `src/lib.rs` or write a small binary to instantiate a
`SimNetwork` and call `run_until()`.

## Extending NAT models
Implement the `NatModel` trait in `nat.rs` and plug it into `SimPeer::new`.

