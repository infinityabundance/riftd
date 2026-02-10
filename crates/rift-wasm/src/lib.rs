//! WebAssembly bindings for the Rift protocol.
//!
//! This module exposes a minimal API for:
//! - invite creation/inspection
//! - session bootstrap
//! - encrypted text encode/decode using protocol framing
//! - voice frame encode/decode for browser audio integration
//! - audio utilities (level metering, VAD)

use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use js_sys::{Date, Uint8Array};
use rift_core::{
    invite::{decode_invite, encode_invite, generate_invite, Invite},
    Identity,
};
use rift_protocol::{
    decode_frame, encode_frame, ChatMessage, CodecId, EncryptedPayload, ProtocolVersion,
    RiftFrameHeader, RiftPayload, SessionId, StreamKind, VoicePacket,
};
use serde::Serialize;
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

#[derive(Serialize)]
struct DecodedVoiceFrame {
    from: String,
    timestamp: u64,
    seq: u32,
    codec: String,
    payload: Vec<u8>,
}

/// Audio configuration for browser integration.
#[wasm_bindgen]
pub struct AudioConfig {
    /// Sample rate in Hz (typically 48000 for Opus).
    sample_rate: u32,
    /// Number of channels (1 for mono, 2 for stereo).
    channels: u8,
    /// Frame size in samples per channel.
    frame_size: u32,
}

#[wasm_bindgen]
impl AudioConfig {
    /// Create a new audio configuration.
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32, channels: u8, frame_size: u32) -> Self {
        Self {
            sample_rate,
            channels,
            frame_size,
        }
    }

    /// Create default config for Opus (48kHz, mono, 20ms frame).
    #[wasm_bindgen]
    pub fn opus_default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 1,
            frame_size: 960, // 20ms at 48kHz
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[wasm_bindgen(getter)]
    pub fn channels(&self) -> u8 {
        self.channels
    }

    #[wasm_bindgen(getter)]
    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    /// Calculate frame duration in milliseconds.
    #[wasm_bindgen]
    pub fn frame_duration_ms(&self) -> f64 {
        (self.frame_size as f64 / self.sample_rate as f64) * 1000.0
    }
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

    /// Encode a voice frame into an encrypted Rift frame.
    ///
    /// The `opus_payload` should be pre-encoded Opus data from a browser
    /// Opus encoder (e.g., via AudioWorklet + opus-wasm).
    #[wasm_bindgen]
    pub fn encode_voice(&mut self, opus_payload: Uint8Array) -> Result<Uint8Array, JsValue> {
        let timestamp = now_ms();
        let voice = VoicePacket {
            codec_id: CodecId::Opus,
            payload: opus_payload.to_vec(),
        };
        let payload = RiftPayload::Voice(voice);
        let encrypted = self.encrypt_payload(&payload)?;
        let header = RiftFrameHeader {
            version: ProtocolVersion::V2,
            stream: StreamKind::Voice,
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

    /// Encode a voice frame with raw PCM16 samples.
    ///
    /// Use this when you have raw Int16Array samples from Web Audio API.
    /// The samples will be wrapped in a PCM16 codec frame.
    #[wasm_bindgen]
    pub fn encode_voice_pcm(&mut self, pcm_samples: Uint8Array) -> Result<Uint8Array, JsValue> {
        let timestamp = now_ms();
        let voice = VoicePacket {
            codec_id: CodecId::PCM16,
            payload: pcm_samples.to_vec(),
        };
        let payload = RiftPayload::Voice(voice);
        let encrypted = self.encrypt_payload(&payload)?;
        let header = RiftFrameHeader {
            version: ProtocolVersion::V2,
            stream: StreamKind::Voice,
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

    /// Decode an encrypted Rift voice frame.
    ///
    /// Returns a JS object with: from, timestamp, seq, codec, payload.
    /// The payload is the encoded audio data (Opus or PCM16).
    #[wasm_bindgen]
    pub fn decode_voice(&self, bytes: Uint8Array) -> Result<JsValue, JsValue> {
        let data = bytes.to_vec();
        let (header, payload) =
            decode_frame(&data).map_err(|err| WasmError::FrameDecode(err.to_string()))?;
        let decrypted = self.decrypt_payload(&payload)?;
        let RiftPayload::Voice(voice) = decrypted else {
            return Err(WasmError::PayloadDecode("not a voice payload".to_string()).into());
        };
        let codec = match voice.codec_id {
            CodecId::Opus => "opus".to_string(),
            CodecId::PCM16 => "pcm16".to_string(),
            CodecId::Experimental(id) => format!("experimental-{}", id),
        };
        let decoded = DecodedVoiceFrame {
            from: header.source.to_hex(),
            timestamp: header.timestamp,
            seq: header.seq,
            codec,
            payload: voice.payload,
        };
        serde_wasm_bindgen::to_value(&decoded).map_err(|err| err.into())
    }

    /// Get the voice payload bytes from a decoded frame.
    ///
    /// This is a convenience method for extracting just the audio payload
    /// without the metadata, useful for feeding directly to a decoder.
    #[wasm_bindgen]
    pub fn extract_voice_payload(&self, bytes: Uint8Array) -> Result<Uint8Array, JsValue> {
        let data = bytes.to_vec();
        let (_, payload) =
            decode_frame(&data).map_err(|err| WasmError::FrameDecode(err.to_string()))?;
        let decrypted = self.decrypt_payload(&payload)?;
        let RiftPayload::Voice(voice) = decrypted else {
            return Err(WasmError::PayloadDecode("not a voice payload".to_string()).into());
        };
        Ok(Uint8Array::from(voice.payload.as_slice()))
    }

    /// Get the current sequence number.
    #[wasm_bindgen(getter)]
    pub fn seq(&self) -> u32 {
        self.seq
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

// ============================================================
// Audio Utility Functions for Browser Integration
// ============================================================

/// Calculate the RMS audio level from PCM16 samples.
///
/// Takes a Uint8Array of little-endian i16 samples (as from Web Audio).
/// Returns a normalized level from 0.0 (silent) to 1.0 (max).
#[wasm_bindgen]
pub fn audio_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0f64;
    for s in samples {
        let v = *s as f64;
        sum += v * v;
    }
    let rms = (sum / samples.len() as f64).sqrt();
    (rms / i16::MAX as f64) as f32
}

/// Calculate the RMS audio level from a Uint8Array of PCM16 bytes.
///
/// The bytes should be little-endian i16 samples.
#[wasm_bindgen]
pub fn audio_level_bytes(bytes: Uint8Array) -> f32 {
    let data = bytes.to_vec();
    if data.len() < 2 {
        return 0.0;
    }
    let samples: Vec<i16> = data
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    audio_level(&samples)
}

/// Check if an audio frame is "active" (contains speech).
///
/// Simple energy-based VAD: returns true if average amplitude exceeds threshold.
/// Threshold is tuned for typical voice activity.
#[wasm_bindgen]
pub fn is_voice_active(samples: &[i16]) -> bool {
    if samples.is_empty() {
        return false;
    }
    let mut sum = 0i64;
    for s in samples {
        sum += (*s as i64).abs();
    }
    let avg = sum / samples.len() as i64;
    avg > 250
}

/// Check if an audio frame is "active" from PCM16 bytes.
#[wasm_bindgen]
pub fn is_voice_active_bytes(bytes: Uint8Array) -> bool {
    let data = bytes.to_vec();
    if data.len() < 2 {
        return false;
    }
    let samples: Vec<i16> = data
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    is_voice_active(&samples)
}

/// Convert Float32Array audio samples to PCM16 bytes.
///
/// Useful for converting Web Audio API float samples to PCM16 format.
/// Input should be normalized floats in range [-1.0, 1.0].
#[wasm_bindgen]
pub fn float32_to_pcm16(samples: &[f32]) -> Uint8Array {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    Uint8Array::from(bytes.as_slice())
}

/// Convert PCM16 bytes to Float32 samples.
///
/// Useful for feeding decoded audio to Web Audio API.
/// Returns normalized floats in range [-1.0, 1.0].
#[wasm_bindgen]
pub fn pcm16_to_float32(bytes: Uint8Array) -> js_sys::Float32Array {
    let data = bytes.to_vec();
    let samples: Vec<f32> = data
        .chunks_exact(2)
        .map(|chunk| {
            let pcm = i16::from_le_bytes([chunk[0], chunk[1]]);
            pcm as f32 / i16::MAX as f32
        })
        .collect();
    js_sys::Float32Array::from(samples.as_slice())
}

/// Compute audio level in decibels (dB) from RMS level.
///
/// Returns dB relative to full scale (0 dB = max amplitude).
/// Silent audio returns -100.0 dB.
#[wasm_bindgen]
pub fn level_to_db(level: f32) -> f32 {
    if level <= 0.0 {
        return -100.0;
    }
    20.0 * level.log10()
}

/// Apply a simple gain to PCM16 samples.
///
/// Gain of 1.0 = no change, 2.0 = double amplitude, 0.5 = half amplitude.
/// Values are clamped to prevent clipping.
#[wasm_bindgen]
pub fn apply_gain(bytes: Uint8Array, gain: f32) -> Uint8Array {
    let data = bytes.to_vec();
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(2) {
        let pcm = i16::from_le_bytes([chunk[0], chunk[1]]);
        let amplified = (pcm as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        out.extend_from_slice(&amplified.to_le_bytes());
    }
    Uint8Array::from(out.as_slice())
}

/// Mix two audio frames together.
///
/// Both frames must be the same length. Result is averaged to prevent clipping.
#[wasm_bindgen]
pub fn mix_frames(frame_a: Uint8Array, frame_b: Uint8Array) -> Result<Uint8Array, JsValue> {
    let a = frame_a.to_vec();
    let b = frame_b.to_vec();
    if a.len() != b.len() {
        return Err(JsValue::from_str("frames must be same length"));
    }
    let mut out = Vec::with_capacity(a.len());
    for (chunk_a, chunk_b) in a.chunks_exact(2).zip(b.chunks_exact(2)) {
        let pcm_a = i16::from_le_bytes([chunk_a[0], chunk_a[1]]) as i32;
        let pcm_b = i16::from_le_bytes([chunk_b[0], chunk_b[1]]) as i32;
        let mixed = ((pcm_a + pcm_b) / 2).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out.extend_from_slice(&mixed.to_le_bytes());
    }
    Ok(Uint8Array::from(out.as_slice()))
}
