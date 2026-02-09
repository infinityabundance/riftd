# Predictive Rendezvous Simulation Harness

This directory contains experimental simulation harnesses for Predictive Rendezvous. These simulations are designed to explore NAT behaviors and topology effects in a controlled setting. Results are exploratory and should not be treated as guarantees.

## NAT Models
The harness simulates:
- **Port-preserving NAT**
- **Symmetric NAT with hashing**
- **Random port allocation**
- **Mapping timeouts**

Each model is deterministic with configurable parameters.

## Running the Harness
Use the lightweight simulation binary in `rift-rndzv`:
```
cargo run -p rift-rndzv --bin pr-sim
```

It writes a CSV event log to:
```
docs/simulations/pr_sim_example.csv
```
The checked-in CSV is illustrative only.

## Interpreting Results
- `slots_attempted` indicates how many slot computations were executed.
- `delivered=true` indicates a simulated probe reached its target through the NAT model.
- Compare slot durations, window sizes, and NAT behaviors to identify fragile parameter ranges.

## Notes
Simulation outputs are intended for tuning defaults and identifying pathological cases. They are not part of the prior-art claims.
