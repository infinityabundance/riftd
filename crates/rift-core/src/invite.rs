use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::CoreError;
use base64::Engine;

const INVITE_PREFIX: &str = "rift://join/";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub channel_name: String,
    pub password: Option<String>,
    pub channel_key: [u8; 32],
    pub known_peers: Vec<SocketAddr>,
    pub version: u8,
    pub created_at: u64,
}

pub fn generate_invite(
    channel_name: &str,
    password: Option<&str>,
    known_peers: Vec<SocketAddr>,
) -> Invite {
    let mut channel_key = [0u8; 32];
    OsRng.fill_bytes(&mut channel_key);
    Invite {
        channel_name: channel_name.to_string(),
        password: password.map(|s| s.to_string()),
        channel_key,
        known_peers,
        version: 1,
        created_at: now_timestamp(),
    }
}

pub fn encode_invite(invite: &Invite) -> String {
    let bytes = bincode::serialize(invite).expect("serialize invite");
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("{}{}", INVITE_PREFIX, encoded)
}

pub fn decode_invite(url: &str) -> Result<Invite, CoreError> {
    if !url.starts_with(INVITE_PREFIX) {
        return Err(CoreError::InvalidInvite);
    }
    let payload = &url[INVITE_PREFIX.len()..];
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| CoreError::InvalidInvite)?;
    let invite: Invite = bincode::deserialize(&bytes)?;
    Ok(invite)
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
