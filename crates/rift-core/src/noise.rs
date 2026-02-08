use crate::CoreError;

/// Noise pattern and cipher suite used for all Rift sessions.
/// XX provides mutual authentication with static keys learned during the handshake,
/// and ChaChaPoly+BLAKE2s provides AEAD encryption and hashing.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

pub fn noise_builder() -> snow::Builder<'static> {
    let params: snow::params::NoiseParams = NOISE_PATTERN.parse().expect("valid noise params");
    snow::Builder::new(params)
}

pub struct NoiseSession {
    state: snow::TransportState,
}

impl NoiseSession {
    pub fn new(state: snow::TransportState) -> Self {
        Self { state }
    }

    pub fn encrypt(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, CoreError> {
        Ok(self.state.write_message(plaintext, out)?)
    }

    pub fn decrypt(&mut self, ciphertext: &[u8], out: &mut [u8]) -> Result<usize, CoreError> {
        Ok(self.state.read_message(ciphertext, out)?)
    }
}

impl Drop for NoiseSession {
    fn drop(&mut self) {
        // Best-effort zeroization: snow doesn't expose explicit key erasure,
        // so we overwrite the TransportState memory before drop.
        unsafe {
            std::ptr::write_bytes(
                &mut self.state as *mut snow::TransportState as *mut u8,
                0,
                std::mem::size_of::<snow::TransportState>(),
            );
        }
    }
}
