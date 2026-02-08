//! Rift Protocol: versioned, framed on-the-wire messages for P2P voice + text.
//!
//! A Rift frame is:
//! - magic: 4 bytes ("RFT1")
//! - version: u8
//! - frame_len: u32 (length of the encoded frame body)
//! - frame body: bincode-encoded `(RiftFrameHeader, RiftPayload)`
//!
//! Versioning: each peer advertises supported protocol versions. The highest
//! common version is selected for communication.
//!
//! Streams: frames declare a `StreamKind` (Control / Text / Voice / Custom)
//! to allow multiplexing and future extensions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use rift_core::{ChannelId, MessageId, PeerId};
use rand::rngs::OsRng;
use rand::RngCore;

const MAGIC: &[u8; 4] = b"RFT1";
const MAX_FRAME_LEN: usize = 64 * 1024;

/// On-the-wire protocol versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ProtocolVersion {
    V1 = 1,
    V2 = 2,
}

impl ProtocolVersion {
    /// Convert to the raw u8 that is encoded on the wire.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Parse from a raw u8, returning None if unsupported.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(ProtocolVersion::V1),
            2 => Some(ProtocolVersion::V2),
            _ => None,
        }
    }
}

/// Logical stream classification used to multiplex payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamKind {
    Voice,
    Text,
    Control,
    Custom(u16),
}

/// Audio codec identifiers used for voice frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecId {
    Opus,
    PCM16,
    Experimental(u16),
}

/// Feature flags advertised during capability exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureFlag {
    Voice,
    Text,
    Relay,
    E2EE,
    ScreenShare,
    DataChannel,
}

/// Session identifier derived from channel metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub [u8; 32]);

impl SessionId {
    /// An all-zero sentinel session id (unused for real sessions).
    pub const NONE: SessionId = SessionId([0u8; 32]);

    /// Generate a random session id.
    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SessionId(bytes)
    }

    /// Derive a deterministic session id from channel name + optional password.
    pub fn from_channel(name: &str, password: Option<&str>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rift-channel:");
        hasher.update(name.as_bytes());
        if let Some(password) = password {
            hasher.update(b":");
            hasher.update(password.as_bytes());
        }
        let hash = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hash.as_bytes());
        SessionId(bytes)
    }

    /// Convert to hex for logs and UI.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// Header for every framed message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftFrameHeader {
    /// Negotiated protocol version for this session.
    pub version: ProtocolVersion,
    /// Stream classification (text, voice, control, etc).
    pub stream: StreamKind,
    /// Custom flags for future expansions.
    pub flags: u16,
    /// Monotonic sequence number for the sender.
    pub seq: u32,
    /// Sender timestamp in milliseconds.
    pub timestamp: u64,
    /// Sender peer id.
    pub source: PeerId,
    /// Session id the frame belongs to.
    pub session: SessionId,
}

/// High-level chat message payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Content-addressed message id.
    pub id: MessageId,
    /// Sender peer id.
    pub from: PeerId,
    /// Sender timestamp.
    pub timestamp: u64,
    /// Raw message text.
    pub text: String,
}

impl ChatMessage {
    /// Construct a new message and compute its id.
    pub fn new(from: PeerId, timestamp: u64, text: String) -> Self {
        let id = MessageId::new(from, timestamp, &text);
        Self {
            id,
            from,
            timestamp,
            text,
        }
    }
}

/// Capability advertisement payload for negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Supported protocol versions in preference order.
    pub supported_versions: Vec<ProtocolVersion>,
    /// Supported audio codecs.
    pub audio_codecs: Vec<CodecId>,
    /// Feature flags (voice, text, E2EE, relay, etc).
    pub features: Vec<FeatureFlag>,
    /// Optional bitrate ceiling.
    pub max_bitrate: Option<u32>,
    /// Preferred audio frame duration.
    pub preferred_frame_duration_ms: Option<u16>,
}

/// Call session lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallState {
    Ringing,
    Active,
    Ended,
}

/// Group topology mode for multi-peer calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupMode {
    Mesh,
    Hybrid { forwarder: PeerId },
}

pub type StreamId = u64;

/// Group-level control messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupControl {
    Join { session: SessionId, peer_id: PeerId },
    Leave { session: SessionId, peer_id: PeerId },
    StreamPublish {
        session: SessionId,
        stream_id: StreamId,
        from: PeerId,
        codec: CodecId,
    },
    StreamSubscribe {
        session: SessionId,
        stream_id: StreamId,
        from: PeerId,
    },
    Topology {
        session: SessionId,
        mode: GroupMode,
    },
}

/// One-to-one call control messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallControl {
    Invite {
        session: SessionId,
        from: PeerId,
        to: PeerId,
        display_name: Option<String>,
    },
    Accept { session: SessionId, from: PeerId },
    Decline {
        session: SessionId,
        from: PeerId,
        reason: Option<String>,
    },
    Bye { session: SessionId, from: PeerId },
    Mute {
        session: SessionId,
        from: PeerId,
        muted: bool,
    },
    SessionInfo {
        session: SessionId,
        participants: Vec<PeerId>,
    },
}

/// ICE-lite candidate classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CandidateType {
    Host,
    Srflx,
    Relay,
}

/// ICE-lite candidate structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    /// Socket address for the candidate.
    pub addr: std::net::SocketAddr,
    /// Candidate type (host/srflx/relay).
    pub cand_type: CandidateType,
    /// Priority used for selection heuristics.
    pub priority: u32,
    /// Foundation group for candidate correlation.
    pub foundation: u64,
}

/// Control plane messages exchanged over the Control stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    Join { peer_id: PeerId, display_name: Option<String> },
    Hello {
        peer_id: PeerId,
        public_key: Vec<u8>,
        capabilities: Capabilities,
        #[serde(default)]
        candidates: Vec<std::net::SocketAddr>,
    },
    IceCandidates {
        peer_id: PeerId,
        session: SessionId,
        candidates: Vec<IceCandidate>,
    },
    IceCheck {
        session: SessionId,
        tie_breaker: u64,
        candidate: IceCandidate,
    },
    IceCheckAck {
        session: SessionId,
        candidate: IceCandidate,
    },
    KeyInit {
        session: SessionId,
        eph_pub_x25519: [u8; 32],
        sig_ed25519: Vec<u8>,
    },
    KeyResp {
        session: SessionId,
        eph_pub_x25519: [u8; 32],
        sig_ed25519: Vec<u8>,
    },
    EncryptedReady {
        session: SessionId,
        alg: u8,
    },
    Leave { peer_id: PeerId },
    PeerState { peer_id: PeerId, relay_capable: bool },
    Chat(ChatMessage),
    Ping { nonce: u64, sent_at_ms: u64 },
    Pong { nonce: u64, sent_at_ms: u64 },
    Auth { token: Vec<u8> },
    RouteInfo { from: PeerId, to: PeerId, relayed: bool },
    Capabilities(Capabilities),
    CapabilitiesUpdate(Capabilities),
    PeerList { peers: Vec<PeerInfo> },
    Call(CallControl),
    Group(GroupControl),
    E2eeInit {
        session: SessionId,
        from: PeerId,
        public_key: [u8; 32],
        signature: Vec<u8>,
    },
    E2eeResp {
        session: SessionId,
        from: PeerId,
        public_key: [u8; 32],
        signature: Vec<u8>,
    },
}

/// Encrypted payload wrapper for E2EE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// AEAD nonce (12 bytes for AES-GCM).
    pub nonce: [u8; 12],
    /// Ciphertext bytes.
    pub ciphertext: Vec<u8>,
}

/// Peer metadata used in peer lists and routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer id.
    pub peer_id: PeerId,
    /// Primary address.
    pub addr: std::net::SocketAddr,
    /// Optional additional addresses.
    #[serde(default)]
    pub addrs: Vec<std::net::SocketAddr>,
    /// Relay capability flag.
    pub relay_capable: bool,
}

/// Voice packet payload (pre-Opus or Opus-encoded audio data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePacket {
    /// Codec for this packet.
    pub codec_id: CodecId,
    /// Encoded audio payload.
    pub payload: Vec<u8>,
}

/// QoS profile for adaptive audio tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosProfile {
    /// Target end-to-end latency.
    pub target_latency_ms: u32,
    /// Maximum tolerated latency.
    pub max_latency_ms: u32,
    /// Minimum bitrate to preserve intelligibility.
    pub min_bitrate: u32,
    /// Maximum bitrate to avoid saturation.
    pub max_bitrate: u32,
    /// Packet loss tolerance threshold.
    pub packet_loss_tolerance: f32,
}

impl Default for QosProfile {
    fn default() -> Self {
        Self {
            target_latency_ms: 50,
            max_latency_ms: 200,
            min_bitrate: 16_000,
            max_bitrate: 96_000,
            packet_loss_tolerance: 0.08,
        }
    }
}

/// The union of all possible payloads for a Rift frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiftPayload {
    Control(ControlMessage),
    Voice(VoicePacket),
    Text(ChatMessage),
    Relay { target: PeerId, inner: Box<RiftPayload> },
    Encrypted(EncryptedPayload),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_control_roundtrip() {
        let session = SessionId::random();
        let msg = ControlMessage::Group(GroupControl::Topology {
            session,
            mode: GroupMode::Hybrid {
                forwarder: PeerId([7u8; 32]),
            },
        });
        let bytes = bincode::serialize(&msg).expect("serialize");
        let decoded: ControlMessage = bincode::deserialize(&bytes).expect("deserialize");
        match decoded {
            ControlMessage::Group(GroupControl::Topology { session: s, mode }) => {
                assert_eq!(s, session);
                assert!(matches!(mode, GroupMode::Hybrid { .. }));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_oversize_frames() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(ProtocolVersion::V1.as_u8());
        let len = (MAX_FRAME_LEN + 1) as u32;
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.resize(10, 0);
        let res = decode_frame(&bytes);
        assert!(matches!(res, Err(FrameError::FrameTooLarge)));
    }
}

/// Errors encountered when decoding protocol frames.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("invalid magic")]
    InvalidMagic,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),
    #[error("frame length mismatch")]
    LengthMismatch,
    #[error("frame too large")]
    FrameTooLarge,
    #[error("decode error: {0}")]
    Decode(#[from] bincode::Error),
}

/// Return the protocol versions supported by this build.
pub fn supported_versions() -> &'static [ProtocolVersion] {
    &[ProtocolVersion::V2, ProtocolVersion::V1]
}

/// Select the highest mutual version between two peers.
pub fn select_version(theirs: &[ProtocolVersion]) -> Option<ProtocolVersion> {
    let mut ours = supported_versions().to_vec();
    ours.sort();
    let mut theirs = theirs.to_vec();
    theirs.sort();
    ours.into_iter()
        .rev()
        .find(|v| theirs.contains(v))
}

/// Encode a framed message into bytes:
/// magic + version + length + bincode(body).
pub fn encode_frame(header: &RiftFrameHeader, payload: &RiftPayload) -> Vec<u8> {
    let body = bincode::serialize(&(header, payload)).expect("serialize frame");
    let mut out = Vec::with_capacity(4 + 1 + 4 + body.len());
    out.extend_from_slice(MAGIC);
    out.push(header.version.as_u8());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Decode a framed message from bytes, validating length and magic.
pub fn decode_frame(bytes: &[u8]) -> Result<(RiftFrameHeader, RiftPayload), FrameError> {
    if bytes.len() < 9 {
        return Err(FrameError::LengthMismatch);
    }
    if &bytes[..4] != MAGIC {
        return Err(FrameError::InvalidMagic);
    }
    let version = ProtocolVersion::from_u8(bytes[4]).ok_or(FrameError::UnsupportedVersion(bytes[4]))?;
    let len = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]) as usize;
    if len > MAX_FRAME_LEN {
        return Err(FrameError::FrameTooLarge);
    }
    if bytes.len() < 9 + len {
        return Err(FrameError::LengthMismatch);
    }
    let body = &bytes[9..9 + len];
    let (mut header, payload): (RiftFrameHeader, RiftPayload) = bincode::deserialize(body)?;
    header.version = version;
    Ok((header, payload))
}
