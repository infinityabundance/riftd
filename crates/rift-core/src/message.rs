//! Identifier types used across the protocol and storage layers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    /// Encode the peer id as hex for logging and UI.
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
    /// Derive a channel id from a channel name and optional password.
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

    /// Encode the channel id as hex.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub [u8; 32]);

impl MessageId {
    /// Create a message id by hashing sender + timestamp + text.
    /// This provides stable ids across transports.
    pub fn new(from: PeerId, timestamp: u64, text: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&from.0);
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();
        MessageId(*hash.as_bytes())
    }
}
