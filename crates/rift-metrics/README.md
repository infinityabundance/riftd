# rift-metrics

Metrics collection and reporting for Rift P2P.

## Features

- Connection statistics tracking
- Latency and throughput metrics
- NAT traversal success rates
- Thread-safe metric collection

## Usage

```rust
use rift_metrics::Metrics;

let metrics = Metrics::global();
metrics.record_connection();
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
