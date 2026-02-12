# Changelog

## 0.1.0 (Unreleased)

### Added
- P2P mesh voice + text chat over UDP.
- LAN discovery (mDNS), invite-based internet joins, and DHT discovery.
- NAT traversal: UDP hole punching + STUN candidates + optional TURN fallback.
- Peer relay fallback with auto-upgrade to direct routes.
- Pairwise end-to-end encryption for chat and voice.
- QoS adaptation and link stats reporting.
- SDK (Rust + C FFI) for embedding Rift.
- TUI client with call/session semantics and PTT.
- Android and Qt desktop clients (early/experimental).
- Metrics and observability hooks.

### Changed
- Versioned protocol framing and capabilities negotiation.
- Session/call control semantics on the wire.

### Security
- TOFU identity verification with known_hosts.
- Optional shared secret channel auth.
- Audit logging for security events.

### Notes
- TURN is optional and intended for self-hosting.
- Group calls use a hybrid mesh/relay topology and are still evolving.
