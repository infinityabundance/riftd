# Rift Protocol
This document describes the on-the-wire framing and message types used by Rift.

## Framing
Every UDP packet that carries Rift protocol data uses a simple binary frame:

```
[magic: 4 bytes]   "RFT1"
[version: u8]      protocol version (currently 1)
[frame_len: u32]   length of the encoded body
[body bytes]       bincode-encoded (RiftFrameHeader, RiftPayload)
```

The body is encoded with `bincode` and contains:
- `RiftFrameHeader`
- `RiftPayload`

## Header
`RiftFrameHeader` includes:
- `version: ProtocolVersion`
- `stream: StreamKind`
- `flags: u16`
- `seq: u32`
- `timestamp: u64`
- `source: PeerId`
- `session: SessionId`

`session` allows a single mesh to support multiple call sessions.

## Streams
`StreamKind`:
- `Control`
- `Text`
- `Voice`
- `Custom(u16)`

Streams are used for multiplexing and future extensions.

## Payloads
`RiftPayload` is one of:
- `Control(ControlMessage)`
- `Text(ChatMessage)`
- `Voice(VoicePacket)`
- `Relay { target, inner }`
- `Encrypted(EncryptedPayload)`

### ControlMessage
Core control messages:
- `Join { peer_id, display_name }`
- `Hello { peer_id, public_key, capabilities, candidates }`
- `IceCandidates { peer_id, session, candidates }`
- `IceCheck { session, tie_breaker, candidate }`
- `IceCheckAck { session, candidate }`
- `KeyInit { session, eph_pub_x25519, sig_ed25519 }`
- `KeyResp { session, eph_pub_x25519, sig_ed25519 }`
- `Leave { peer_id }`
- `PeerState { peer_id, relay_capable }`
- `PeerList { peers }`
- `Chat(ChatMessage)`
- `RouteInfo { from, to, relayed }`
- `Capabilities(Capabilities)`
- `Call(CallControl)`

### PeerInfo
```
PeerInfo {
  peer_id: PeerId,
  addr: SocketAddr,
  addrs: Vec<SocketAddr>, // candidate endpoints (public + local)
  relay_capable: bool
}
```

### IceCandidate
```
IceCandidate {
  addr: SocketAddr,
  cand_type: CandidateType,  // Host | Srflx | Relay
  priority: u32,
  foundation: u64
}
```

### ChatMessage
```
ChatMessage {
  id: MessageId,
  from: PeerId,
  timestamp: u64,
  text: String
}
```

### VoicePacket
```
VoicePacket {
  codec_id: u8,   // 1 = Opus
  payload: Vec<u8>
}
```

### CallControl
Call/session state over the mesh:
- `Invite { session, from, to, display_name }`
- `Accept { session, from }`
- `Decline { session, from, reason }`
- `Bye { session, from }`
- `Mute { session, from, muted }`
- `SessionInfo { session, participants }`

### Relay
Relayed messages:
- `Relay { target, inner }`
- A relay peer forwards `inner` to the final `target`.

### EncryptedPayload
When end-to-end encryption is enabled, chat and voice payloads may be wrapped:
```
EncryptedPayload {
  alg: u8,        // 1 = ChaCha20-Poly1305
  nonce: [u8; 12],
  ciphertext: Vec<u8>
}
```
The ciphertext is an AEAD-encrypted `RiftPayload` using a channel/session key.

## E2EE Key Exchange
Rift uses X25519 for ephemeral key agreement and Ed25519 for authentication.
Ephemeral keys are signed with the long-term Ed25519 identity key:

```
KeyInit {
  session: SessionId,
  eph_pub_x25519: [u8; 32],
  sig_ed25519: [u8; 64]
}

KeyResp {
  session: SessionId,
  eph_pub_x25519: [u8; 32],
  sig_ed25519: [u8; 64]
}
```

Peers derive a session key using X25519 ECDH + HKDF and then wrap
chat/voice payloads as `EncryptedPayload`.

## ICE-lite Candidate Exchange
Peers exchange candidate sets using `IceCandidates`. Connectivity checks use
`IceCheck`/`IceCheckAck` to select the best path:
- Prefer direct > punched (srflx) > relayed > TURN.

These messages are optional and ignored by older peers.

## Versioning
Supported protocol versions are listed in `supported_versions()`.
Negotiation:
- Each peer can advertise supported versions (future capability message).
- The highest common version is selected.

Backward compatibility:
- V1 peers ignore unknown ControlMessage variants.
- E2EE is only enabled when both peers support key exchange.

## Session Semantics
- The channel name maps to a deterministic `SessionId` (channel session).
- Ad-hoc calls create a random `SessionId`.
- Voice packets are tagged with `session` in the frame header.

## Encryption
Protocol frames are carried inside Noise-encrypted UDP messages. The protocol
described here is the plaintext payload of the Noise transport.

## Stability
This protocol is still evolving. The crate `rift-protocol` should be treated
as the canonical source of truth for on-the-wire behavior.
