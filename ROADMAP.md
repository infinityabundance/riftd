# Roadmap
This roadmap outlines likely next steps. It is not a guarantee, but it reflects
the direction of the project.

## Near Term
1. **Protocol hardening**
   - Capability exchange on connect.
   - Version negotiation and downgrade.
   - Formalized error/ack messages.

2. **Public internet reachability**
   - STUN-based public address discovery.
   - ICE-lite candidate exchange + connectivity checks.
   - Keep-alives and retry logic for NAT reliability.

3. **Call UX**
   - Group call management in the TUI.
   - Dedicated call list and participant indicators.
   - Muting per participant.

4. **Audio quality improvements**
   - Adaptive jitter buffer based on measured latency.
   - Comfort noise / packet loss concealment tuning.
   - Optional echo cancellation.

5. **Discovery reliability**
   - Better LAN presence detection (timeouts, reconnection).
   - Invite-based peer refresh and endpoint revalidation.

## Mid Term
1. **Mesh scalability**
   - Smarter relay selection.
   - Partial-mesh topologies.

2. **Security**
   - Noise pattern selection + rotation.
   - Signed peer metadata.

3. **SDK/API layer**
   - Provide an API surface for third-party apps.
   - Stable framing + FFI support.

## Long Term
1. **Interoperability**
   - Formal protocol specification and test vectors.
2. **Multi-device**
   - Identity sync across devices.
3. **Optional infra**
   - Introduce lightweight, optional coordination services (still P2P-first).
