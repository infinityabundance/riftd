use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("snow error: {0}")]
    Snow(#[from] snow::Error),
    #[error("invalid key length")]
    InvalidKeyLength,
    #[error("identity not found at {0}")]
    IdentityMissing(String),
    #[error("config directory not found")]
    ConfigDirMissing,
}
