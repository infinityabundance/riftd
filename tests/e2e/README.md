# E2E Tests

This directory holds the shared E2E test definitions. The actual test crate
is `crates/rift-e2e`, which includes this module.

## Running

```bash
cargo test -p rift-e2e
```

## Notes

Some scenarios (TURN or netem) are marked `#[ignore]` and require
explicit opt-in.
