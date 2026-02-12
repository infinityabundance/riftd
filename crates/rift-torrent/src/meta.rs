//! Torrent metadata types: TorrentMeta, InfoHash, PieceHash, FileInfo.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 20-byte SHA1 or 32-byte SHA256 infohash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InfoHash {
    /// BEP-3 SHA1 hash (20 bytes).
    Sha1([u8; 20]),
    /// BEP-52 SHA256 hash (32 bytes).
    Sha256([u8; 32]),
}

impl InfoHash {
    /// Encode as hex string for display and magnet URIs.
    pub fn to_hex(&self) -> String {
        match self {
            InfoHash::Sha1(h) => hex::encode(h),
            InfoHash::Sha256(h) => hex::encode(h),
        }
    }

    /// Convert to 32-byte array (SHA1 is zero-padded).
    pub fn to_bytes32(&self) -> [u8; 32] {
        match self {
            InfoHash::Sha1(h) => {
                let mut buf = [0u8; 32];
                buf[..20].copy_from_slice(h);
                buf
            }
            InfoHash::Sha256(h) => *h,
        }
    }

    /// Get the raw bytes of the hash.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            InfoHash::Sha1(h) => h,
            InfoHash::Sha256(h) => h,
        }
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self, crate::TorrentError> {
        let bytes = hex::decode(s).map_err(|_| crate::TorrentError::InfoHash("invalid hex"))?;
        match bytes.len() {
            20 => {
                let mut h = [0u8; 20];
                h.copy_from_slice(&bytes);
                Ok(InfoHash::Sha1(h))
            }
            32 => {
                let mut h = [0u8; 32];
                h.copy_from_slice(&bytes);
                Ok(InfoHash::Sha256(h))
            }
            _ => Err(crate::TorrentError::InfoHash("invalid length (expected 20 or 32 bytes)")),
        }
    }
}

/// Per-piece verification hash (SHA1 per BEP-3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceHash(pub [u8; 20]);

impl PieceHash {
    /// Create from raw bytes.
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

/// Single file within a torrent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    /// Relative path components.
    pub path: PathBuf,
    /// File length in bytes.
    pub length: u64,
}

/// SRT extension parameters stored in torrent file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SrtExtension {
    /// Version of the SRT extension format.
    pub version: u8,
    /// Offset from torrent creation time for t0.
    pub t0_offset: u64,
    /// Window duration in seconds.
    pub window_secs: u64,
    /// Slot duration in milliseconds.
    pub slot_ms: u64,
    /// Optional salt for seed derivation.
    pub salt: Option<[u8; 16]>,
}

impl Default for SrtExtension {
    fn default() -> Self {
        Self {
            version: 1,
            t0_offset: 0,
            window_secs: 300,  // 5 minutes
            slot_ms: 500,      // 500ms slots
            salt: None,
        }
    }
}

/// Parsed .torrent file with optional SRT extension.
#[derive(Clone, Debug)]
pub struct TorrentMeta {
    /// Primary infohash (SHA1 or SHA256).
    pub info_hash: InfoHash,
    /// Display name.
    pub name: String,
    /// Piece length in bytes.
    pub piece_length: u64,
    /// Per-piece hashes for verification.
    pub pieces: Vec<PieceHash>,
    /// File list (single or multi-file).
    pub files: Vec<FileInfo>,
    /// Total torrent size in bytes.
    pub total_length: u64,
    /// Traditional tracker URL (optional, for fallback).
    pub announce: Option<String>,
    /// Tiered tracker list.
    pub announce_list: Vec<Vec<String>>,
    /// Creation timestamp (Unix seconds).
    pub creation_date: Option<u64>,
    /// SRT extension (None for traditional torrents).
    pub srt_extension: Option<SrtExtension>,
    /// Comment field.
    pub comment: Option<String>,
    /// Created by field.
    pub created_by: Option<String>,
}

impl TorrentMeta {
    /// Check if this torrent has SRT extension enabled.
    pub fn has_srt(&self) -> bool {
        self.srt_extension.is_some()
    }

    /// Get the total number of pieces.
    pub fn piece_count(&self) -> u32 {
        self.pieces.len() as u32
    }

    /// Get the expected length of the last piece.
    pub fn last_piece_length(&self) -> u64 {
        if self.pieces.is_empty() || self.piece_length == 0 {
            return 0;
        }
        let remainder = self.total_length % self.piece_length;
        if remainder == 0 {
            self.piece_length
        } else {
            remainder
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infohash_hex_roundtrip_sha1() {
        let hash = InfoHash::Sha1([0xAB; 20]);
        let hex = hash.to_hex();
        let parsed = InfoHash::from_hex(&hex).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn infohash_hex_roundtrip_sha256() {
        let hash = InfoHash::Sha256([0xCD; 32]);
        let hex = hash.to_hex();
        let parsed = InfoHash::from_hex(&hex).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn infohash_to_bytes32_padding() {
        let hash = InfoHash::Sha1([0xFF; 20]);
        let bytes = hash.to_bytes32();
        assert_eq!(&bytes[..20], &[0xFF; 20]);
        assert_eq!(&bytes[20..], &[0x00; 12]);
    }

    #[test]
    fn invalid_hex_length() {
        let result = InfoHash::from_hex("abcd");
        assert!(result.is_err());
    }
}
