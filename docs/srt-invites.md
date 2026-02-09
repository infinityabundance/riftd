# SRT Invites

SRT invites are a lightweight UX wrapper around a Semantic Rendezvous Token (SRT).
They are meant for human sharing (chat, email, QR) without altering the SRT format.

## CLI usage
Generate an invite targeted at a specific peer:
```bash
rift rndzv invite --label "Voice Call" --peer-id <peer-id-hex>
```

Generate a generic invite (no identity constraints) with a QR code:
```bash
rift rndzv invite --label "Pairing" --qr
```

Inspect the underlying SRT:
```bash
rift rndzv inspect <srt-uri>
```

Output JSON for embedding in other tools:
```bash
rift rndzv invite --label "Voice Call" --peer-id <peer-id-hex> --json
```

## SDK usage
Create and accept invites without manually touching SRT encoding:
```rust
use rift_sdk::{create_voice_invite, accept_voice_invite, PeerId};

let invite = create_voice_invite(PeerId([0u8; 32]));
let srt = accept_voice_invite(&invite)?;
```

## Notes
- Invites are presentation-layer conveniences; they do not define protocol semantics.
- SRTs still contain no IPs or ports.
- QR output is an optional CLI rendering of the same SRT URI string.
