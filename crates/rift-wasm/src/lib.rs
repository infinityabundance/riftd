//! WebAssembly bindings for the Rift protocol.
//!
//! This module exposes a minimal API for:
//! - invite creation/inspection
//! - session bootstrap
//! - encrypted text encode/decode using protocol framing

use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use js_sys::{Date, Uint8Array};
use rift_core::{
    invite::{decode_invite, encode_invite, generate_invite, Invite},
    Identity,
};
use rift_protocol::{
    decode_frame, encode_frame, ChatMessage, EncryptedPayload, ProtocolVersion, RiftFrameHeader,
    RiftPayload, SessionId, StreamKind,
};
use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use thiserror::Error;
use wasm_bindgen::prelude::*;

#[derive(Debug, Error)]
enum WasmError {
    #[error("invalid invite: {0}")]
    InvalidInvite(String),
    #[error("frame decode failed: {0}")]
    FrameDecode(String),
    #[error("cipher error")]
    Cipher,
    #[error("payload decode failed: {0}")]
    PayloadDecode(String),
}

impl From<WasmError> for JsValue {
    fn from(err: WasmError) -> Self {
        JsValue::from_str(&err.to_string())
    }
}

#[wasm_bindgen]
pub struct WasmClient {
    /// Ephemeral identity for the session.
    identity: Identity,
    /// Session identifier derived from the invite.
    session: SessionId,
    /// Symmetric channel key for AES-GCM.
    channel_key: [u8; 32],
    /// Local sequence counter for frames.
    seq: u32,
}

#[wasm_bindgen]
pub struct InviteInfo {
    /// Channel name embedded in the invite.
    channel_name: String,
    /// Whether a password was set.
    has_password: bool,
    /// Protocol version.
    version: u8,
    /// Invite creation timestamp.
    created_at: u64,
}

#[wasm_bindgen]
impl InviteInfo {
    #[wasm_bindgen(getter)]
    pub fn channel_name(&self) -> String {
        self.channel_name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn has_password(&self) -> bool {
        self.has_password
    }

    #[wasm_bindgen(getter)]
    pub fn version(&self) -> u8 {
        self.version
    }

    #[wasm_bindgen(getter)]
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
}

#[derive(Serialize)]
struct DecodedTextMessage {
    from: String,
    timestamp: u64,
    text: String,
}

#[wasm_bindgen]
pub fn create_invite(channel_name: String, password: Option<String>) -> Result<String, JsValue> {
    let invite = generate_invite(
        &channel_name,
        password.as_deref(),
        Vec::new(),
        Vec::new(),
    );
    Ok(encode_invite(&invite))
}

#[wasm_bindgen]
pub fn inspect_invite(invite_url: String) -> Result<InviteInfo, JsValue> {
    let invite = decode_invite(&invite_url)
        .map_err(|err| WasmError::InvalidInvite(err.to_string()))?;
    Ok(InviteInfo {
        channel_name: invite.channel_name,
        has_password: invite.password.is_some(),
        version: invite.version,
        created_at: invite.created_at,
    })
}

#[wasm_bindgen]
pub fn join_invite(invite_url: String) -> Result<WasmClient, JsValue> {
    let invite = decode_invite(&invite_url)
        .map_err(|err| WasmError::InvalidInvite(err.to_string()))?;
    Ok(WasmClient::from_invite(invite))
}

#[wasm_bindgen]
impl WasmClient {
    /// Construct a client from an invite.
    fn from_invite(invite: Invite) -> Self {
        let identity = Identity::generate();
        let session = SessionId::from_channel(&invite.channel_name, invite.password.as_deref());
        Self {
            identity,
            session,
            channel_key: invite.channel_key,
            seq: 0,
        }
    }

    /// Return this client's peer id as hex.
    #[wasm_bindgen(getter)]
    pub fn peer_id(&self) -> String {
        self.identity.peer_id.to_hex()
    }

    /// Return the session id as hex.
    #[wasm_bindgen(getter)]
    pub fn session_id(&self) -> String {
        self.session.to_hex()
    }

    /// Encode a text message into an encrypted Rift frame.
    #[wasm_bindgen]
    pub fn encode_text(&mut self, text: String) -> Result<Uint8Array, JsValue> {
        let timestamp = now_ms();
        let message = ChatMessage::new(self.identity.peer_id, timestamp, text);
        let payload = RiftPayload::Text(message);
        let encrypted = self.encrypt_payload(&payload)?;
        let header = RiftFrameHeader {
            version: ProtocolVersion::V2,
            stream: StreamKind::Text,
            flags: 0,
            seq: self.seq,
            timestamp,
            source: self.identity.peer_id,
            session: self.session,
        };
        self.seq = self.seq.wrapping_add(1);
        let frame = encode_frame(&header, &encrypted);
        Ok(Uint8Array::from(frame.as_slice()))
    }

    /// Decode an encrypted Rift frame into a JSON-compatible JS object.
    #[wasm_bindgen]
    pub fn decode_text(&self, bytes: Uint8Array) -> Result<JsValue, JsValue> {
        let data = bytes.to_vec();
        let (_, payload) =
            decode_frame(&data).map_err(|err| WasmError::FrameDecode(err.to_string()))?;
        let decrypted = self.decrypt_payload(&payload)?;
        let RiftPayload::Text(message) = decrypted else {
            return Err(WasmError::PayloadDecode("not a text payload".to_string()).into());
        };
        let decoded = DecodedTextMessage {
            from: message.from.to_hex(),
            timestamp: message.timestamp,
            text: message.text,
        };
        let serializer = Serializer::json_compatible();
        serde_wasm_bindgen::to_value(&decoded).map_err(|err| err.into())
    }

    /// Encrypt a payload using the channel key.
    fn encrypt_payload(&self, payload: &RiftPayload) -> Result<RiftPayload, JsValue> {
        let serialized = bincode::serialize(payload)
            .map_err(|err| WasmError::PayloadDecode(err.to_string()))?;
        let cipher = Aes256Gcm::new_from_slice(&self.channel_key)
            .map_err(|_| WasmError::Cipher)?;
        let nonce_bytes = random_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, serialized.as_ref())
            .map_err(|_| WasmError::Cipher)?;
        Ok(RiftPayload::Encrypted(EncryptedPayload {
            nonce: nonce_bytes,
            ciphertext,
        }))
    }

    /// Decrypt a payload using the channel key.
    fn decrypt_payload(&self, payload: &RiftPayload) -> Result<RiftPayload, JsValue> {
        let RiftPayload::Encrypted(encrypted) = payload else {
            return Err(WasmError::PayloadDecode("missing encrypted payload".to_string()).into());
        };
        let cipher = Aes256Gcm::new_from_slice(&self.channel_key)
            .map_err(|_| WasmError::Cipher)?;
        let nonce = Nonce::from_slice(&encrypted.nonce);
        let plaintext = cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|_| WasmError::Cipher)?;
        let decoded: RiftPayload = bincode::deserialize(&plaintext)
            .map_err(|err| WasmError::PayloadDecode(err.to_string()))?;
        Ok(decoded)
    }
}

/// Current time in milliseconds (JS Date).
fn now_ms() -> u64 {
    Date::now() as u64
}

/// Generate a random AES-GCM nonce.
fn random_nonce() -> [u8; 12] {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).expect("random nonce");
    nonce
}
