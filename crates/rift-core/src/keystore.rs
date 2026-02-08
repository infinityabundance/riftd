//! On-disk identity storage and rotation.
//!
//! The keystore is a native-only utility. WASM targets return explicit
//! "unsupported" errors and should provide their own storage abstractions
//! (e.g., IndexedDB or in-memory).

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Keypair, PublicKey};
use rand::rngs::OsRng;
use thiserror::Error;

use crate::{CoreError, Identity};

#[derive(Debug, Error)]
pub enum KeyStoreError {
    /// I/O error reading or writing identity material.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Corrupted identity file or incorrect key length.
    #[error("invalid key length")]
    InvalidKeyLength,
    /// Missing identity file.
    #[error("identity not found at {0}")]
    IdentityMissing(String),
    /// Wrapped error from core helpers.
    #[error("core error: {0}")]
    Core(#[from] CoreError),
}

pub struct KeyStore {
    /// Path to the current identity file.
    path: PathBuf,
    /// Cached identity material.
    identity: Identity,
}

impl KeyStore {
    #[cfg(not(target_arch = "wasm32"))]
    /// Load an identity or create it if it doesn't exist.
    pub fn load_or_generate(path: &Path) -> Result<Identity, KeyStoreError> {
        match Identity::load(Some(path)) {
            Ok(identity) => Ok(identity),
            Err(CoreError::IdentityMissing(_)) => {
                let mut rng = OsRng;
                let keypair = Keypair::generate(&mut rng);
                let identity = Identity::generate_from_keypair(keypair);
                identity.save(path)?;
                Ok(identity)
            }
            Err(err) => Err(KeyStoreError::Core(err)),
        }
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets are ephemeral: always generate a fresh identity.
    pub fn load_or_generate(_path: &Path) -> Result<Identity, KeyStoreError> {
        Ok(Identity::generate())
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Open an existing keystore at a specific path.
    pub fn open(path: &Path) -> Result<Self, KeyStoreError> {
        let identity = Identity::load(Some(path)).map_err(KeyStoreError::Core)?;
        Ok(Self {
            path: path.to_path_buf(),
            identity,
        })
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets cannot open a file-backed keystore.
    pub fn open(_path: &Path) -> Result<Self, KeyStoreError> {
        Err(KeyStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "keystore unsupported on wasm",
        )))
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Rotate the identity by archiving the current key and generating a new one.
    pub fn rotate(&mut self) -> Result<(), KeyStoreError> {
        let old_dir = self.old_dir();
        fs::create_dir_all(&old_dir)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let archived = old_dir.join(format!("identity-{ts}.key"));
        fs::rename(&self.path, archived)?;

        let mut rng = OsRng;
        let keypair = Keypair::generate(&mut rng);
        let identity = Identity::generate_from_keypair(keypair);
        identity.save(&self.path)?;
        self.identity = identity;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets cannot rotate a file-backed keystore.
    pub fn rotate(&mut self) -> Result<(), KeyStoreError> {
        Err(KeyStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "keystore unsupported on wasm",
        )))
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Return all known public keys, including archived ones.
    pub fn list_public_keys(&self) -> Vec<PublicKey> {
        let mut keys = Vec::new();
        if let Ok(keypair) = read_keypair(&self.path) {
            keys.push(keypair.public);
        }
        if let Ok(entries) = fs::read_dir(self.old_dir()) {
            for entry in entries.flatten() {
                if let Ok(keypair) = read_keypair(&entry.path()) {
                    keys.push(keypair.public);
                }
            }
        }
        keys
    }

    #[cfg(target_arch = "wasm32")]
    /// WASM targets only have the in-memory identity.
    pub fn list_public_keys(&self) -> Vec<PublicKey> {
        vec![self.identity.keypair.public]
    }

    /// Borrow the current identity.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Directory for archived keys.
    fn old_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".old")
    }

    #[cfg(target_arch = "wasm32")]
    /// No archive directory in WASM.
    fn old_dir(&self) -> PathBuf {
        PathBuf::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Read a keypair from disk with length validation.
fn read_keypair(path: &Path) -> Result<Keypair, KeyStoreError> {
    let bytes = fs::read(path)?;
    if bytes.len() != 64 {
        return Err(KeyStoreError::InvalidKeyLength);
    }
    Keypair::from_bytes(&bytes).map_err(|_| KeyStoreError::InvalidKeyLength)
}
