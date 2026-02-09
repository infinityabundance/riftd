//! Noise protocol helpers.
//!
//! This module centralizes the Noise pattern selection and provides a small
//! wrapper (`NoiseSession`) around the `snow` transport state.

use crate::CoreError;

/// Noise pattern and cipher suite used for all Rift sessions.
/// XX provides mutual authentication with static keys learned during the handshake,
/// and ChaChaPoly+BLAKE2s provides AEAD encryption and hashing.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Create a Noise builder configured with the Rift pattern.
pub fn noise_builder() -> snow::Builder<'static> {
    let params: snow::params::NoiseParams = NOISE_PATTERN.parse().expect("valid noise params");
    snow::Builder::new(params)
}

/// Wrapper around a Noise transport state that provides encrypt/decrypt helpers.
pub struct NoiseSession {
    state: snow::TransportState,
}

impl NoiseSession {
    /// Construct a new session from a fully-negotiated transport state.
    pub fn new(state: snow::TransportState) -> Self {
        Self { state }
    }

    /// Encrypt a plaintext message into the provided output buffer.
    pub fn encrypt(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, CoreError> {
        Ok(self.state.write_message(plaintext, out)?)
    }

    /// Decrypt a ciphertext message into the provided output buffer.
    pub fn decrypt(&mut self, ciphertext: &[u8], out: &mut [u8]) -> Result<usize, CoreError> {
        Ok(self.state.read_message(ciphertext, out)?)
    }
}

// NOTE: We intentionally avoid unsafe zeroization here because `snow::TransportState`
// may contain heap pointers. A safer zeroize strategy can be added when available.
