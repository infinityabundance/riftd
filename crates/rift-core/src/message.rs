use serde::{Deserialize, Serialize};

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
