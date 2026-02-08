//! Invite encoding/decoding and helper utilities.
//!
//! Invites encapsulate enough metadata to bootstrap a session:
//! - channel identity (name + optional password)
//! - per-channel encryption key
//! - known peers and ICE-style candidates

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::CoreError;
use base64::Engine;

/// URI prefix for invites.
const INVITE_PREFIX: &str = "rift://join/";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    /// Human-readable channel name.
    pub channel_name: String,
    /// Optional channel password.
    pub password: Option<String>,
    /// Symmetric channel key used for message encryption bootstrap.
    pub channel_key: [u8; 32],
    /// Known peer endpoints supplied at invite creation time.
    pub known_peers: Vec<SocketAddr>,
    /// Candidate endpoints discovered by ICE-lite routines.
    #[serde(default)]
    pub candidates: Vec<SocketAddr>,
    /// Optional relay endpoints (TURN or peer relay).
    #[serde(default)]
    pub relay_candidates: Vec<SocketAddr>,
    /// Protocol version encoded into the invite.
    pub version: u8,
    /// Creation timestamp in seconds since epoch.
    pub created_at: u64,
}

/// Create a new invite with a fresh channel key.
pub fn generate_invite(
    channel_name: &str,
    password: Option<&str>,
    known_peers: Vec<SocketAddr>,
    candidates: Vec<SocketAddr>,
) -> Invite {
    let mut channel_key = [0u8; 32];
    OsRng.fill_bytes(&mut channel_key);
    Invite {
        channel_name: channel_name.to_string(),
        password: password.map(|s| s.to_string()),
        channel_key,
        known_peers,
        candidates,
        relay_candidates: Vec::new(),
        version: 2,
        created_at: now_timestamp(),
    }
}

/// Encode the invite as a URL-safe string.
pub fn encode_invite(invite: &Invite) -> String {
    let bytes = bincode::serialize(invite).expect("serialize invite");
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("{}{}", INVITE_PREFIX, encoded)
}

/// Decode an invite string back to the structured form.
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

/// Timestamp helper for invite creation.
fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
