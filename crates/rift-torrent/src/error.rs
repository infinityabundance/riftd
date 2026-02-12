//! Error types for rift-torrent.

use thiserror::Error;

/// Errors that can occur in rift-torrent operations.
#[derive(Error, Debug)]
pub enum TorrentError {
    /// Bencode parsing/encoding errors.
    #[error("bencode error: {0}")]
    Bencode(String),

    /// Invalid .torrent file structure.
    #[error("invalid torrent file: {0}")]
    InvalidTorrent(&'static str),

    /// Magnet URI parsing errors.
    #[error("invalid magnet uri: {0}")]
    InvalidMagnet(&'static str),

    /// SRT parsing or derivation errors.
    #[error("srt error: {0}")]
    Srt(String),

    /// I/O errors (file operations).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Peer discovery failed.
    #[error("peer discovery failed: {0}")]
    Discovery(String),

    /// Piece verification failed.
    #[error("piece hash mismatch at index {0}")]
    PieceHashMismatch(u32),

    /// Infohash derivation error.
    #[error("infohash error: {0}")]
    InfoHash(&'static str),

    /// URL encoding/decoding error.
    #[error("url encoding error: {0}")]
    UrlEncoding(String),
}

impl From<hex::FromHexError> for TorrentError {
    fn from(_: hex::FromHexError) -> Self {
        TorrentError::InfoHash("invalid hex encoding")
    }
}
