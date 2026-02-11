//! WebAssembly browser chat client for Rift over WebSocket relay.
//!
//! This crate provides a simple API for text-only chat in the browser,
//! using encrypted Rift protocol frames over a WebSocket relay.
//!
//! # Example (JavaScript)
//!
//! ```js
//! import init, { WebChat, create_invite } from 'rift-web-chat';
//!
//! await init();
//!
//! const invite = create_invite("my-room", null);
//! const chat = new WebChat("ws://localhost:8787/ws", invite);
//!
//! chat.on_message((msg) => {
//!     console.log(`${msg.from}: ${msg.text}`);
//! });
//!
//! chat.on_connect(() => {
//!     chat.send("Hello, world!");
//! });
//! ```

mod callbacks;
mod relay;

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use callbacks::{invoke_callback_event, ChatMessageEvent, ConnectionEvent, PeerEvent};
use js_sys::Date;
use relay::RelayEnvelope;
use rift_core::{
    invite::{decode_invite, encode_invite, generate_invite, Invite},
    Identity,
};
use rift_protocol::{
    decode_frame, encode_frame, ChatMessage, EncryptedPayload, ProtocolVersion, RiftFrameHeader,
    RiftPayload, SessionId, StreamKind,
};
use std::cell::RefCell;
use std::rc::Rc;
use thiserror::Error;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

#[derive(Debug, Error)]
#[allow(dead_code)]
enum ChatError {
    #[error("invalid invite: {0}")]
    InvalidInvite(String),
    #[error("websocket error: {0}")]
    WebSocket(String),
    #[error("not connected")]
    NotConnected,
    #[error("frame decode error: {0}")]
    FrameDecode(String),
    #[error("encryption error")]
    Cipher,
    #[error("payload decode error: {0}")]
    PayloadDecode(String),
}

impl From<ChatError> for JsValue {
    fn from(err: ChatError) -> Self {
        JsValue::from_str(&err.to_string())
    }
}

/// Internal state for the WebChat client.
struct ChatState {
    identity: Identity,
    session: SessionId,
    channel_key: [u8; 32],
    room: String,
    seq: u32,
    on_message: Option<js_sys::Function>,
    on_peer_event: Option<js_sys::Function>,
    on_connect: Option<js_sys::Function>,
    on_disconnect: Option<js_sys::Function>,
    on_error: Option<js_sys::Function>,
}

impl ChatState {
    fn from_invite(invite: Invite) -> Self {
        let identity = Identity::generate();
        let session = SessionId::from_channel(&invite.channel_name, invite.password.as_deref());
        Self {
            identity,
            session,
            channel_key: invite.channel_key,
            room: invite.channel_name,
            seq: 0,
        on_message: None,
            on_peer_event: None,
            on_connect: None,
            on_disconnect: None,
            on_error: None,
        }
    }

    fn encrypt_payload(&self, payload: &RiftPayload) -> Result<RiftPayload, ChatError> {
        let serialized = bincode::serialize(payload).map_err(|e| ChatError::PayloadDecode(e.to_string()))?;
        let cipher = Aes256Gcm::new_from_slice(&self.channel_key).map_err(|_| ChatError::Cipher)?;
        let nonce_bytes = random_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, serialized.as_ref())
            .map_err(|_| ChatError::Cipher)?;
        Ok(RiftPayload::Encrypted(EncryptedPayload {
            nonce: nonce_bytes,
            ciphertext,
        }))
    }

    fn decrypt_payload(&self, payload: &RiftPayload) -> Result<RiftPayload, ChatError> {
        let RiftPayload::Encrypted(encrypted) = payload else {
            return Err(ChatError::PayloadDecode("expected encrypted payload".into()));
        };
        let cipher = Aes256Gcm::new_from_slice(&self.channel_key).map_err(|_| ChatError::Cipher)?;
        let nonce = Nonce::from_slice(&encrypted.nonce);
        let plaintext = cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|_| ChatError::Cipher)?;
        bincode::deserialize(&plaintext).map_err(|e| ChatError::PayloadDecode(e.to_string()))
    }

    fn encode_text(&mut self, text: &str) -> Result<Vec<u8>, ChatError> {
        let timestamp = now_ms();
        let message = ChatMessage::new(self.identity.peer_id, timestamp, text.to_string());
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
        Ok(encode_frame(&header, &encrypted))
    }

    fn decode_text(&self, data: &[u8]) -> Result<ChatMessageEvent, ChatError> {
        let (_, payload) = decode_frame(data).map_err(|e| ChatError::FrameDecode(e.to_string()))?;
        let decrypted = self.decrypt_payload(&payload)?;
        let RiftPayload::Text(message) = decrypted else {
            return Err(ChatError::PayloadDecode("not a text message".into()));
        };
        Ok(ChatMessageEvent {
            from: message.from.to_hex(),
            timestamp: message.timestamp,
            text: message.text,
        })
    }
}

/// WebSocket-based chat client for the browser.
///
/// Connects to a `rift-ws-relay` server and provides encrypted text chat.
#[wasm_bindgen]
pub struct WebChat {
    socket: WebSocket,
    state: Rc<RefCell<ChatState>>,
    // Keep closures alive
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

#[wasm_bindgen]
impl WebChat {
    /// Create a new WebChat client and connect to the relay.
    ///
    /// # Arguments
    /// * `relay_url` - WebSocket URL of the relay (e.g., "ws://localhost:8787/ws")
    /// * `invite_url` - Rift invite URL (e.g., "rift://z/...")
    #[wasm_bindgen(constructor)]
    pub fn new(relay_url: &str, invite_url: &str) -> Result<WebChat, JsValue> {
        let invite =
            decode_invite(invite_url).map_err(|e| ChatError::InvalidInvite(e.to_string()))?;

        let state = Rc::new(RefCell::new(ChatState::from_invite(invite)));
        let socket = WebSocket::new(relay_url).map_err(|e| ChatError::WebSocket(format!("{:?}", e)))?;
        socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Clone state for closures
        let state_open = state.clone();
        let socket_open = socket.clone();
        let on_open = Closure::new(move || {
            let s = state_open.borrow();
            // Send join message
            let join = RelayEnvelope::join(&s.session.to_hex(), &s.identity.peer_id.to_hex());
            if let Ok(json) = join.to_json() {
                let _ = socket_open.send_with_str(&json);
            }
            // Notify connect callback
            if let Some(cb) = &s.on_connect {
                invoke_callback_event(cb, &ConnectionEvent {
                    state: "connected".into(),
                    error: None,
                });
            }
        });

        let state_msg = state.clone();
        let on_message = Closure::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                handle_relay_message(&state_msg, &text);
            }
        });

        let state_err = state.clone();
        let on_error = Closure::new(move |event: ErrorEvent| {
            let s = state_err.borrow();
            if let Some(cb) = &s.on_error {
                invoke_callback_event(cb, &ConnectionEvent {
                    state: "error".into(),
                    error: Some(event.message()),
                });
            }
        });

        let state_close = state.clone();
        let on_close = Closure::new(move |_event: CloseEvent| {
            let s = state_close.borrow();
            if let Some(cb) = &s.on_disconnect {
                invoke_callback_event(cb, &ConnectionEvent {
                    state: "disconnected".into(),
                    error: None,
                });
            }
        });

        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(WebChat {
            socket,
            state,
            _on_open: on_open,
            _on_message: on_message,
            _on_error: on_error,
            _on_close: on_close,
        })
    }

    /// Send a text message to the chat room.
    pub fn send(&mut self, text: &str) -> Result<(), JsValue> {
        let frame = {
            let mut state = self.state.borrow_mut();
            state.encode_text(text)?
        };

        let state = self.state.borrow();
        let envelope = RelayEnvelope::data(
            &state.session.to_hex(),
            &state.identity.peer_id.to_hex(),
            &frame,
        );
        let json = envelope.to_json().map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.socket
            .send_with_str(&json)
            .map_err(|e| ChatError::WebSocket(format!("{:?}", e)))?;

        Ok(())
    }

    /// Set callback for incoming chat messages.
    ///
    /// Callback receives: `{ from: string, timestamp: number, text: string }`
    pub fn on_message(&mut self, callback: js_sys::Function) {
        self.state.borrow_mut().on_message = Some(callback);
    }

    /// Set callback for peer join/leave events.
    ///
    /// Callback receives: `{ peer_id: string, event: "join" | "leave" }`
    pub fn on_peer_event(&mut self, callback: js_sys::Function) {
        self.state.borrow_mut().on_peer_event = Some(callback);
    }

    /// Set callback for successful connection.
    ///
    /// Callback receives: `{ state: "connected", error: null }`
    pub fn on_connect(&mut self, callback: js_sys::Function) {
        self.state.borrow_mut().on_connect = Some(callback);
    }

    /// Set callback for disconnection.
    ///
    /// Callback receives: `{ state: "disconnected", error: null }`
    pub fn on_disconnect(&mut self, callback: js_sys::Function) {
        self.state.borrow_mut().on_disconnect = Some(callback);
    }

    /// Set callback for errors.
    ///
    /// Callback receives: `{ state: "error", error: string }`
    pub fn on_error(&mut self, callback: js_sys::Function) {
        self.state.borrow_mut().on_error = Some(callback);
    }

    /// Get this client's peer ID (hex-encoded).
    #[wasm_bindgen(getter)]
    pub fn peer_id(&self) -> String {
        self.state.borrow().identity.peer_id.to_hex()
    }

    /// Get the room/channel name.
    #[wasm_bindgen(getter)]
    pub fn room(&self) -> String {
        self.state.borrow().room.clone()
    }

    /// Get the session ID (hex-encoded).
    #[wasm_bindgen(getter)]
    pub fn session_id(&self) -> String {
        self.state.borrow().session.to_hex()
    }

    /// Disconnect from the relay.
    pub fn disconnect(&self) {
        let _ = self.socket.close();
    }

    /// Check if the WebSocket is open.
    #[wasm_bindgen(getter)]
    pub fn is_connected(&self) -> bool {
        self.socket.ready_state() == WebSocket::OPEN
    }
}

fn handle_relay_message(state: &Rc<RefCell<ChatState>>, text: &str) {
    let Ok(envelope) = RelayEnvelope::from_json(text) else {
        return;
    };

    let s = state.borrow();
    let my_peer_id = s.identity.peer_id.to_hex();

    match &envelope {
        RelayEnvelope::Data { peer_id, .. } => {
            // Skip our own messages
            if peer_id == &my_peer_id {
                return;
            }

            if let Some(data) = envelope.decode_data() {
                if let Ok(msg) = s.decode_text(&data) {
                    if let Some(cb) = &s.on_message {
                        invoke_callback_event(cb, &msg);
                    }
                }
            }
        }
        RelayEnvelope::Status { peer_id, status, .. } => {
            if let Some(cb) = &s.on_peer_event {
                invoke_callback_event(cb, &PeerEvent {
                    peer_id: peer_id.clone(),
                    event: status.clone(),
                });
            }
        }
        RelayEnvelope::Join { peer_id, .. } => {
            // Another peer joined
            if peer_id != &my_peer_id {
                if let Some(cb) = &s.on_peer_event {
                    invoke_callback_event(cb, &PeerEvent {
                        peer_id: peer_id.clone(),
                        event: "join".into(),
                    });
                }
            }
        }
    }
}

/// Generate a random 12-byte nonce for AES-GCM.
fn random_nonce() -> [u8; 12] {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).expect("random nonce");
    nonce
}

/// Current time in milliseconds.
fn now_ms() -> u64 {
    Date::now() as u64
}

// ============================================================
// Standalone utility functions
// ============================================================

/// Create a new invite URL for a chat room.
#[wasm_bindgen]
pub fn create_invite(channel_name: &str, password: Option<String>) -> String {
    let invite = generate_invite(channel_name, password.as_deref(), Vec::new(), Vec::new());
    encode_invite(&invite)
}

/// Inspect an invite URL without joining.
///
/// Returns a JS object with: `{ channel_name, has_password, version, created_at }`
#[wasm_bindgen]
pub fn inspect_invite(invite_url: &str) -> Result<JsValue, JsValue> {
    let invite =
        decode_invite(invite_url).map_err(|e| JsValue::from_str(&format!("Invalid invite: {}", e)))?;

    #[derive(serde::Serialize)]
    struct InviteInfo {
        channel_name: String,
        has_password: bool,
        version: u8,
        created_at: u64,
    }

    let info = InviteInfo {
        channel_name: invite.channel_name,
        has_password: invite.password.is_some(),
        version: invite.version,
        created_at: invite.created_at,
    };

    serde_wasm_bindgen::to_value(&info).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Generate a new random identity and return the peer ID (hex-encoded).
#[wasm_bindgen]
pub fn generate_peer_id() -> String {
    Identity::generate().peer_id.to_hex()
}
