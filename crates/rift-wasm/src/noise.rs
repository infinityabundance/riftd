//! Noise protocol handshake for pairwise E2EE in browser clients.
//!
//! This module implements the Noise XX pattern for establishing pairwise
//! encryption between browser clients, replacing the insecure shared key approach.

use snow::{Builder, params::NoiseParams};
use wasm_bindgen::prelude::*;

/// Noise protocol pattern: XX
/// XX provides mutual authentication and forward secrecy.
/// Pattern: -> e, <- e, ee, s, es, -> s, se
const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Noise handshake state for WASM clients.
#[wasm_bindgen]
pub struct NoiseHandshake {
    /// Internal snow handshake state
    state: snow::HandshakeState,
}

#[wasm_bindgen]
impl NoiseHandshake {
    /// Create a new initiator handshake.
    /// 
    /// # Arguments
    /// * `static_key` - 32-byte Ed25519 static key for this peer
    #[wasm_bindgen(constructor)]
    pub fn new_initiator(static_key: &[u8]) -> Result<NoiseHandshake, JsValue> {
        if static_key.len() != 32 {
            return Err(JsValue::from_str("static_key must be 32 bytes"));
        }

        let params: NoiseParams = NOISE_PATTERN.parse()
            .map_err(|e| JsValue::from_str(&format!("Invalid Noise params: {}", e)))?;
        
        let builder = Builder::new(params);
        let state = builder
            .local_private_key(static_key)
            .build_initiator()
            .map_err(|e| JsValue::from_str(&format!("Failed to build initiator: {}", e)))?;

        Ok(NoiseHandshake {
            state,
        })
    }

    /// Create a new responder handshake.
    /// 
    /// # Arguments
    /// * `static_key` - 32-byte Ed25519 static key for this peer
    #[wasm_bindgen]
    pub fn new_responder(static_key: &[u8]) -> Result<NoiseHandshake, JsValue> {
        if static_key.len() != 32 {
            return Err(JsValue::from_str("static_key must be 32 bytes"));
        }

        let params: NoiseParams = NOISE_PATTERN.parse()
            .map_err(|e| JsValue::from_str(&format!("Invalid Noise params: {}", e)))?;
        
        let builder = Builder::new(params);
        let state = builder
            .local_private_key(static_key)
            .build_responder()
            .map_err(|e| JsValue::from_str(&format!("Failed to build responder: {}", e)))?;

        Ok(NoiseHandshake {
            state,
        })
    }

    /// Write a handshake message.
    /// 
    /// # Arguments
    /// * `payload` - Optional payload to include in this message
    /// 
    /// # Returns
    /// The handshake message bytes to send to the peer
    #[wasm_bindgen]
    pub fn write_message(&mut self, payload: Option<Vec<u8>>) -> Result<Vec<u8>, JsValue> {
        let payload_bytes = payload.unwrap_or_default();
        let mut buf = vec![0u8; 65535]; // Max handshake message size
        
        let len = self.state
            .write_message(&payload_bytes, &mut buf)
            .map_err(|e| JsValue::from_str(&format!("Failed to write message: {}", e)))?;
        
        buf.truncate(len);
        Ok(buf)
    }

    /// Read a handshake message from the peer.
    /// 
    /// # Arguments
    /// * `message` - The handshake message bytes received from peer
    /// 
    /// # Returns
    /// The decrypted payload (if any)
    #[wasm_bindgen]
    pub fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>, JsValue> {
        let mut buf = vec![0u8; 65535];
        
        let len = self.state
            .read_message(message, &mut buf)
            .map_err(|e| JsValue::from_str(&format!("Failed to read message: {}", e)))?;
        
        buf.truncate(len);
        Ok(buf)
    }

    /// Check if the handshake is complete.
    #[wasm_bindgen]
    pub fn is_handshake_finished(&self) -> bool {
        self.state.is_handshake_finished()
    }

    /// Transition to transport mode after handshake completes.
    /// Returns a NoiseTransport for encrypted communication.
    #[wasm_bindgen]
    pub fn into_transport_mode(self) -> Result<NoiseTransport, JsValue> {
        if !self.state.is_handshake_finished() {
            return Err(JsValue::from_str("Handshake not finished"));
        }

        let transport = self.state
            .into_transport_mode()
            .map_err(|e| JsValue::from_str(&format!("Failed to transition to transport: {}", e)))?;

        Ok(NoiseTransport { state: transport })
    }

    /// Get the remote peer's static public key (after handshake completes).
    #[wasm_bindgen]
    pub fn get_remote_static(&self) -> Result<Vec<u8>, JsValue> {
        self.state
            .get_remote_static()
            .map(|s| s.to_vec())
            .ok_or_else(|| JsValue::from_str("Remote static key not available"))
    }
}

/// Noise transport state for encrypted communication after handshake.
#[wasm_bindgen]
pub struct NoiseTransport {
    state: snow::TransportState,
}

#[wasm_bindgen]
impl NoiseTransport {
    /// Encrypt and send a message.
    /// 
    /// # Arguments
    /// * `plaintext` - The plaintext message to encrypt
    /// 
    /// # Returns
    /// The encrypted ciphertext with authentication tag
    #[wasm_bindgen]
    pub fn send(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, JsValue> {
        let mut buf = vec![0u8; plaintext.len() + 16]; // +16 for auth tag
        
        let len = self.state
            .write_message(plaintext, &mut buf)
            .map_err(|e| JsValue::from_str(&format!("Failed to encrypt: {}", e)))?;
        
        buf.truncate(len);
        Ok(buf)
    }

    /// Decrypt a received message.
    /// 
    /// # Arguments
    /// * `ciphertext` - The encrypted message from peer
    /// 
    /// # Returns
    /// The decrypted plaintext
    #[wasm_bindgen]
    pub fn recv(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, JsValue> {
        let mut buf = vec![0u8; ciphertext.len()];
        
        let len = self.state
            .read_message(ciphertext, &mut buf)
            .map_err(|e| JsValue::from_str(&format!("Failed to decrypt: {}", e)))?;
        
        buf.truncate(len);
        Ok(buf)
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;

    // Note: These tests require WASM test runner like wasm-pack test
    // Run with: wasm-pack test --node

    #[test]
    fn test_noise_handshake_roundtrip() {
        // Generate keys for both peers
        let initiator_key = vec![1u8; 32];
        let responder_key = vec![2u8; 32];

        // Create handshake states
        let mut initiator = NoiseHandshake::new_initiator(&initiator_key).unwrap();
        let mut responder = NoiseHandshake::new_responder(&responder_key).unwrap();

        // Step 1: Initiator -> Responder (e)
        let msg1 = initiator.write_message(None).unwrap();
        let _ = responder.read_message(&msg1).unwrap();

        // Step 2: Responder -> Initiator (e, ee, s, es)
        let msg2 = responder.write_message(None).unwrap();
        let _ = initiator.read_message(&msg2).unwrap();

        // Step 3: Initiator -> Responder (s, se)
        let msg3 = initiator.write_message(None).unwrap();
        let _ = responder.read_message(&msg3).unwrap();

        // Both should be finished now
        assert!(initiator.is_handshake_finished());
        assert!(responder.is_handshake_finished());

        // Transition to transport mode
        let mut initiator_transport = initiator.into_transport_mode().unwrap();
        let mut responder_transport = responder.into_transport_mode().unwrap();

        // Test encrypted communication
        let plaintext = b"Hello from initiator";
        let ciphertext = initiator_transport.send(plaintext).unwrap();
        let decrypted = responder_transport.recv(&ciphertext).unwrap();
        assert_eq!(&decrypted, plaintext);

        // Test reverse direction
        let plaintext2 = b"Hello from responder";
        let ciphertext2 = responder_transport.send(plaintext2).unwrap();
        let decrypted2 = initiator_transport.recv(&ciphertext2).unwrap();
        assert_eq!(&decrypted2, plaintext2);
    }

    #[test]
    fn test_invalid_key_length() {
        let short_key = vec![1u8; 16];
        assert!(NoiseHandshake::new_initiator(&short_key).is_err());
    }

    #[test]
    fn test_handshake_not_finished_error() {
        let key = vec![1u8; 32];
        let handshake = NoiseHandshake::new_initiator(&key).unwrap();
        assert!(handshake.into_transport_mode().is_err());
    }
}
