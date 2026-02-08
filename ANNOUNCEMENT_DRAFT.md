# Rift v0.1.0 Announcement Draft

Today we’re releasing **Rift v0.1.0**, a lightweight, serverless P2P voice + text system focused on real-time collaboration without heavy WebRTC stacks.

## Highlights
- LAN discovery via mDNS + internet joins via invites and DHT
- NAT traversal with UDP hole punching, STUN candidates, and optional TURN fallback
- Peer relay fallback with auto-upgrade to direct paths
- Pairwise end-to-end encryption for chat + voice
- Rust SDK + C FFI for embedding
- TUI client with call/session semantics and PTT

## Quick Links
- GitHub: https://github.com/infinityabundance/riftd
- Docs: PROTOCOL.md, SECURITY.md, TURN_GUIDE.md

## Notes
Rift is early but usable for small groups. If you try it, please file issues with logs and a short repro so we can improve reliability.

## Thank You
Thanks to everyone testing and providing feedback — especially around NAT edge cases and audio QoS tuning.
