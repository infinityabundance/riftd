//! Core error types shared across identity, invites, and crypto helpers.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    /// I/O failure (file read/write, directory creation, etc).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization/deserialization errors for bincode payloads.
    #[error("serialization error: {0}")]
    Bincode(#[from] bincode::Error),
    /// Noise protocol errors (handshake/transport state).
    #[error("snow error: {0}")]
    Snow(#[from] snow::Error),
    /// Key material length mismatch.
    #[error("invalid key length")]
    InvalidKeyLength,
    /// Identity file missing on disk.
    #[error("identity not found at {0}")]
    IdentityMissing(String),
    /// Config directory could not be discovered.
    #[error("config directory not found")]
    ConfigDirMissing,
    /// Invite could not be parsed or failed validation.
    #[error("invalid invite")]
    InvalidInvite,
}
