use crate::CoreError;

pub const NOISE_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

pub fn noise_builder() -> snow::Builder<'static> {
    let params: snow::params::NoiseParams = NOISE_PARAMS.parse().expect("valid noise params");
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
