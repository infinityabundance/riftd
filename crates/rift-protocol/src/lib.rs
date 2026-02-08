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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ProtocolVersion {
    V1 = 1,
}

impl ProtocolVersion {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(ProtocolVersion::V1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamKind {
    Voice,
    Text,
    Control,
    Custom(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecId {
    Opus,
    PCM16,
    Experimental(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureFlag {
    Voice,
    Text,
    Relay,
    ScreenShare,
    DataChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub [u8; 32]);

impl SessionId {
    pub const NONE: SessionId = SessionId([0u8; 32]);

    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SessionId(bytes)
    }

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

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftFrameHeader {
    pub version: ProtocolVersion,
    pub stream: StreamKind,
    pub flags: u16,
    pub seq: u32,
    pub timestamp: u64,
    pub source: PeerId,
    pub session: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: MessageId,
    pub from: PeerId,
    pub timestamp: u64,
    pub text: String,
}

impl ChatMessage {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub supported_versions: Vec<ProtocolVersion>,
    pub audio_codecs: Vec<CodecId>,
    pub features: Vec<FeatureFlag>,
    pub max_bitrate: Option<u32>,
    pub preferred_frame_duration_ms: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallState {
    Ringing,
    Active,
    Ended,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addr: std::net::SocketAddr,
    #[serde(default)]
    pub addrs: Vec<std::net::SocketAddr>,
    pub relay_capable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePacket {
    pub codec_id: CodecId,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosProfile {
    pub target_latency_ms: u32,
    pub max_latency_ms: u32,
    pub min_bitrate: u32,
    pub max_bitrate: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiftPayload {
    Control(ControlMessage),
    Voice(VoicePacket),
    Text(ChatMessage),
    Relay { target: PeerId, inner: Box<RiftPayload> },
    Encrypted(EncryptedPayload),
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("invalid magic")]
    InvalidMagic,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),
    #[error("frame length mismatch")]
    LengthMismatch,
    #[error("decode error: {0}")]
    Decode(#[from] bincode::Error),
}

pub fn supported_versions() -> &'static [ProtocolVersion] {
    &[ProtocolVersion::V1]
}

pub fn select_version(theirs: &[ProtocolVersion]) -> Option<ProtocolVersion> {
    let mut ours = supported_versions().to_vec();
    ours.sort();
    let mut theirs = theirs.to_vec();
    theirs.sort();
    ours.into_iter()
        .rev()
        .find(|v| theirs.contains(v))
}

pub fn encode_frame(header: &RiftFrameHeader, payload: &RiftPayload) -> Vec<u8> {
    let body = bincode::serialize(&(header, payload)).expect("serialize frame");
    let mut out = Vec::with_capacity(4 + 1 + 4 + body.len());
    out.extend_from_slice(MAGIC);
    out.push(header.version.as_u8());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn decode_frame(bytes: &[u8]) -> Result<(RiftFrameHeader, RiftPayload), FrameError> {
    if bytes.len() < 9 {
        return Err(FrameError::LengthMismatch);
    }
    if &bytes[..4] != MAGIC {
        return Err(FrameError::InvalidMagic);
    }
    let version = ProtocolVersion::from_u8(bytes[4]).ok_or(FrameError::UnsupportedVersion(bytes[4]))?;
    let len = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]) as usize;
    if bytes.len() < 9 + len {
        return Err(FrameError::LengthMismatch);
    }
    let body = &bytes[9..9 + len];
    let (mut header, payload): (RiftFrameHeader, RiftPayload) = bincode::deserialize(body)?;
    header.version = version;
    Ok((header, payload))
}
