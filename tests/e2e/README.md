# E2E Tests

This directory holds the shared E2E test definitions. The actual test crate
is `crates/rift-e2e`, which includes this module.

## Running

```bash
cargo test -p rift-e2e
```

## Ignored Tests

Two tests are marked `#[ignore]` and require external infrastructure:

### `nat_restrictive_turn_fallback`

Tests TURN relay fallback when NAT traversal fails.

**Requirements:**
- A running TURN server (e.g., coturn)
- NAT simulation using network namespaces or iptables rules
- Set environment variable: `TURN_SERVER=turn:your-server:3478`
- Set environment variable: `TURN_USERNAME=user`
- Set environment variable: `TURN_PASSWORD=pass`

**Example setup:**
```bash
# Install coturn
sudo apt install coturn

# Start coturn with test credentials
turnserver -n -a -u test:test -r testrealm

# Run the test
TURN_SERVER=turn:127.0.0.1:3478 TURN_USERNAME=test TURN_PASSWORD=test \
  cargo test -p rift-e2e nat_restrictive_turn_fallback -- --ignored
```

### `stun_srflx_connectivity`

Tests STUN server-reflexive address discovery.

**Requirements:**
- Access to public STUN servers (default: Google STUN servers)
- Network access to the internet (not firewalled for UDP 19302)

**Running:**
```bash
# Uses default Google STUN servers
cargo test -p rift-e2e stun_srflx_connectivity -- --ignored

# Or with custom STUN server
STUN_SERVER=stun.example.com:3478 \
  cargo test -p rift-e2e stun_srflx_connectivity -- --ignored
```

## CI Notes

These tests are not run in CI because they require external network access
or dedicated infrastructure. They should be run manually during release
validation or in environments with the required infrastructure.
