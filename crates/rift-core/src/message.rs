use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub [u8; 32]);

impl ChannelId {
    pub fn from_channel(channel: &str, password: Option<&str>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(channel.as_bytes());
        hasher.update(b":");
        if let Some(password) = password {
            hasher.update(password.as_bytes());
        }
        let hash = hasher.finalize();
        ChannelId(*hash.as_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub [u8; 32]);

impl MessageId {
    pub fn new(from: PeerId, timestamp: u64, text: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&from.0);
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();
        MessageId(*hash.as_bytes())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ControlMessage {
    Join { peer_id: PeerId },
    Leave { peer_id: PeerId },
    Chat(ChatMessage),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WireMessage {
    Control(ControlMessage),
    Voice {
        from: PeerId,
        seq: u32,
        timestamp: u64,
        payload: Vec<u8>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CoreMessage {
    Chat { from: PeerId, nonce: u64, text: String },
}

pub fn encode_message(msg: &CoreMessage) -> Vec<u8> {
    bincode::serialize(msg).expect("serialize CoreMessage")
}

pub fn decode_message(bytes: &[u8]) -> Result<CoreMessage, CoreError> {
    Ok(bincode::deserialize(bytes)?)
}

pub fn encode_control_message(msg: &ControlMessage) -> Vec<u8> {
    bincode::serialize(msg).expect("serialize ControlMessage")
}

pub fn decode_control_message(bytes: &[u8]) -> Result<ControlMessage, CoreError> {
    Ok(bincode::deserialize(bytes)?)
}

pub fn encode_wire_message(msg: &WireMessage) -> Vec<u8> {
    bincode::serialize(msg).expect("serialize WireMessage")
}

pub fn decode_wire_message(bytes: &[u8]) -> Result<WireMessage, CoreError> {
    Ok(bincode::deserialize(bytes)?)
}
