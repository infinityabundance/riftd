//! Identity management: keypair generation, peer-id derivation, and persistence.
//!
//! This module is used by both native and WASM clients. Persistence is gated
//! by `cfg(target_arch = "wasm32")` because browsers cannot access the host
//! filesystem directly. For WASM targets, identity is ephemeral unless a higher
//! level chooses to store it (e.g., IndexedDB).

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use ed25519_dalek::Keypair;
use rand::rngs::OsRng;

use crate::{CoreError, PeerId};

#[derive(Debug)]
pub struct Identity {
    /// Long-term Ed25519 identity keypair.
    pub keypair: Keypair,
    /// Stable peer identifier derived from the public key.
    pub peer_id: PeerId,
}

impl Identity {
    /// Generate a new identity using OS randomness.
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let keypair = Keypair::generate(&mut rng);
        Self::generate_from_keypair(keypair)
    }

    /// Wrap an existing keypair and derive its peer id.
    pub fn generate_from_keypair(keypair: Keypair) -> Self {
        let peer_id = peer_id_from_keypair(&keypair);
        Self { keypair, peer_id }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Default on-disk identity path on native targets.
    pub fn default_path() -> Result<PathBuf, CoreError> {
        let base = dirs::config_dir().ok_or(CoreError::ConfigDirMissing)?;
        Ok(base.join("rift").join("identity.key"))
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets cannot resolve a filesystem-backed config directory.
    pub fn default_path() -> Result<PathBuf, CoreError> {
        Err(CoreError::ConfigDirMissing)
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Load an identity from disk or return a detailed error.
    pub fn load(path: Option<&Path>) -> Result<Self, CoreError> {
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => Self::default_path()?,
        };
        let bytes = fs::read(&path)
            .map_err(|err| if err.kind() == std::io::ErrorKind::NotFound {
                CoreError::IdentityMissing(path.display().to_string())
            } else {
                CoreError::Io(err)
            })?;

        if bytes.len() != 64 {
            return Err(CoreError::InvalidKeyLength);
        }
        let keypair = Keypair::from_bytes(&bytes).map_err(|_| CoreError::InvalidKeyLength)?;
        let peer_id = peer_id_from_keypair(&keypair);
        Ok(Self { keypair, peer_id })
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets cannot load from disk; callers should provide their own storage.
    pub fn load(path: Option<&Path>) -> Result<Self, CoreError> {
        let _ = path;
        Err(CoreError::IdentityMissing("identity storage unsupported on wasm".to_string()))
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Load from disk or generate and persist a new identity if missing.
    pub fn load_or_generate(path: Option<&Path>) -> Result<(Self, bool), CoreError> {
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => Self::default_path()?,
        };
        match Self::load(Some(&path)) {
            Ok(identity) => Ok((identity, false)),
            Err(CoreError::IdentityMissing(_)) => {
                let identity = Self::generate();
                identity.save(&path)?;
                Ok((identity, true))
            }
            Err(err) => Err(err),
        }
    }

    #[cfg(target_arch = "wasm32")]
    /// On WASM targets, always generate a new ephemeral identity.
    pub fn load_or_generate(_path: Option<&Path>) -> Result<(Self, bool), CoreError> {
        Ok((Self::generate(), true))
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Persist identity to disk.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = self.keypair.to_bytes();
        fs::write(path, bytes)?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets cannot save to disk; surface an explicit error.
    pub fn save(&self, _path: &Path) -> Result<(), CoreError> {
        Err(CoreError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "identity storage unsupported on wasm",
        )))
    }
}

/// Hash the public key to derive a stable peer id.
fn peer_id_from_keypair(keypair: &Keypair) -> PeerId {
    let mut hasher = Hasher::new();
    hasher.update(keypair.public.as_bytes());
    let hash = hasher.finalize();
    PeerId(*hash.as_bytes())
}

/// Convert raw public key bytes to a peer id, with length validation.
pub fn peer_id_from_public_key_bytes(bytes: &[u8]) -> Result<PeerId, CoreError> {
    if bytes.len() != 32 {
        return Err(CoreError::InvalidKeyLength);
    }
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    Ok(PeerId(*hash.as_bytes()))
}
