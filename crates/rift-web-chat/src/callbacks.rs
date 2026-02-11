//! JavaScript callback management for WebChat events.

use serde::Serialize;
use wasm_bindgen::prelude::*;

/// A chat message received from another peer.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessageEvent {
    /// Hex-encoded peer ID of the sender.
    pub from: String,
    /// Message timestamp in milliseconds since epoch.
    pub timestamp: u64,
    /// The message text.
    pub text: String,
}

/// A peer event (join/leave).
#[derive(Debug, Clone, Serialize)]
pub struct PeerEvent {
    /// Hex-encoded peer ID.
    pub peer_id: String,
    /// Event type: "join" or "leave".
    pub event: String,
}

/// Connection state change event.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionEvent {
    /// Connection state: "connecting", "connected", "disconnected", "error".
    pub state: String,
    /// Optional error message.
    pub error: Option<String>,
}

/// Convert a serializable event to JsValue for callback invocation.
pub fn event_to_js<T: Serialize>(event: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(event).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Invoke a JS callback function with a single argument.
pub fn invoke_callback(callback: &js_sys::Function, arg: JsValue) {
    let _ = callback.call1(&JsValue::NULL, &arg);
}

/// Invoke a JS callback function with an event struct.
pub fn invoke_callback_event<T: Serialize>(callback: &js_sys::Function, event: &T) {
    if let Ok(js_event) = event_to_js(event) {
        invoke_callback(callback, js_event);
    }
}
