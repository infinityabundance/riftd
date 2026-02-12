# Performance Optimization Report (Phase 44)

This report summarizes profiling targets, current bottlenecks, and planned optimizations for v0.1.0 release readiness.

## Profiling Targets
- Audio pipeline: capture → Opus encode → encrypt → send → decrypt → decode → mix → playback.
- Crypto hot path: per-packet AEAD (voice) and control message encryption.
- Mesh routing: candidate checks, relay forwarding, and ICE connectivity checks.

## Observed/Expected Hotspots
- Opus encode/decode dominates CPU on low-power devices.
- Per-packet AEAD contributes measurable overhead at high frame rates.
- Excessive allocations in packet decode/encode and event fan-out.

## Recommended Optimizations
1. **Opus tuning**
   - Prefer 20ms frames, variable bitrate, constrained VBR.
   - Use medium bitrate default with QoS adaptation.

2. **Batching and buffer reuse**
   - Reuse buffers for encode/decode to avoid per-packet allocations.
   - Batch small control packets where possible.

3. **Async scheduling**
   - Use bounded channels for audio frame pipelines to avoid unbounded queue growth.
   - Prioritize voice over chat on congested links.

4. **Metrics & logs**
   - Keep metrics enabled but reduce high-frequency logging in release builds.

## Benchmarks to Track
- End-to-end voice latency (p50/p95) over LAN and internet.
- CPU usage at 2, 5, 10 peers.
- Packet loss impact on QoS adaptation.

## Action Items (pre‑release)
- Add lightweight bench harness for encode/decode + encrypt/decrypt.
- Add a perf smoke test in CI (optional).
- Ensure release builds use `-C opt-level=3` and LTO where appropriate.
