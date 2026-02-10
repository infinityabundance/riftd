# rift-metrics

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  <a href="https://crates.io/crates/rift-metrics"><img src="https://img.shields.io/crates/v/rift-metrics.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rift-metrics"><img src="https://docs.rs/rift-metrics/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/infinityabundance/riftd/blob/main/LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
  Metrics collection and reporting for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-metrics` provides lightweight, thread-safe metrics collection:

- **Connection Stats** — Track active connections and lifetime totals
- **Latency Metrics** — RTT measurements and histograms
- **Throughput** — Bytes sent/received tracking
- **NAT Traversal** — Success/failure rates for hole punching
- **Global Registry** — Thread-safe access via `Metrics::global()`

## Usage

```rust
use rift_metrics::Metrics;

let metrics = Metrics::global();
metrics.record_connection();
metrics.record_bytes_sent(1024);

let snapshot = metrics.snapshot();
println!("Active connections: {}", snapshot.active_connections);
```

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-mesh](https://crates.io/crates/rift-mesh) | Mesh networking (uses metrics) |
| [rift-nat](https://crates.io/crates/rift-nat) | NAT traversal (reports metrics) |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
