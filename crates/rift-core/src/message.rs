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
