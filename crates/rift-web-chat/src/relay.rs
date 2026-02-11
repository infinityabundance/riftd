//! Relay protocol types for WebSocket communication.
//!
//! These types match the JSON envelope format used by `rift-ws-relay`.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Envelope for messages sent to/from the WebSocket relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum RelayEnvelope {
    /// Join a room. Must be the first message sent after connecting.
    Join { room: String, peer_id: String },

    /// Data message containing an encrypted Rift frame (base64-encoded).
    Data {
        room: String,
        peer_id: String,
        data: String,
    },

    /// Status message for peer events (join/leave notifications).
    Status {
        room: String,
        peer_id: String,
        status: String,
    },
}

impl RelayEnvelope {
    /// Create a join message.
    pub fn join(room: &str, peer_id: &str) -> Self {
        Self::Join {
            room: room.to_string(),
            peer_id: peer_id.to_string(),
        }
    }

    /// Create a data message with base64-encoded payload.
    pub fn data(room: &str, peer_id: &str, payload: &[u8]) -> Self {
        use base64::Engine;
        Self::Data {
            room: room.to_string(),
            peer_id: peer_id.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(payload),
        }
    }

    /// Create a status message.
    pub fn status(room: &str, peer_id: &str, status: &str) -> Self {
        Self::Status {
            room: room.to_string(),
            peer_id: peer_id.to_string(),
            status: status.to_string(),
        }
    }

    /// Extract the peer_id from any envelope variant.
    pub fn peer_id(&self) -> &str {
        match self {
            Self::Join { peer_id, .. } => peer_id,
            Self::Data { peer_id, .. } => peer_id,
            Self::Status { peer_id, .. } => peer_id,
        }
    }

    /// Extract the room from any envelope variant.
    pub fn room(&self) -> &str {
        match self {
            Self::Join { room, .. } => room,
            Self::Data { room, .. } => room,
            Self::Status { room, .. } => room,
        }
    }

    /// Decode base64 data payload, if this is a Data message.
    pub fn decode_data(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        match self {
            Self::Data { data, .. } => {
                base64::engine::general_purpose::STANDARD.decode(data).ok()
            }
            _ => None,
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_serialization() {
        let msg = RelayEnvelope::join("test-room", "peer-123");
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"join\""));
        assert!(json.contains("\"room\":\"test-room\""));
        assert!(json.contains("\"peer_id\":\"peer-123\""));
    }

    #[test]
    fn test_data_serialization() {
        let msg = RelayEnvelope::data("room", "peer", b"hello");
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"data\""));

        let parsed = RelayEnvelope::from_json(&json).unwrap();
        assert_eq!(parsed.decode_data().unwrap(), b"hello");
    }
}
