//! SwarmSrt: SRT derivation from infohash for torrent swarms.

use rift_rndzv::{
    EscalationPolicy, IdentityConstraints, RendezvousSpaceId, SearchStrategy,
    SemanticRendezvousToken, TimeModel,
};

use crate::{InfoHash, SrtExtension, TorrentError, TorrentMeta};

/// Default time model parameters for torrent swarms.
pub mod defaults {
    /// Default window duration in seconds (5 minutes).
    pub const WINDOW_SECS: u64 = 300;
    /// Default slot duration in milliseconds.
    pub const SLOT_MS: u64 = 500;
    /// Default t0 offset from creation time.
    pub const T0_OFFSET: u64 = 0;
}

/// SRT configured for a specific torrent swarm.
#[derive(Clone, Debug)]
pub struct SwarmSrt {
    /// The underlying SRT token.
    pub token: SemanticRendezvousToken,
    /// Infohash this SRT is derived from.
    pub info_hash: InfoHash,
    /// Creation timestamp used for t0 derivation.
    pub creation_time: u64,
}

impl SwarmSrt {
    /// Derive SRT from infohash using default parameters.
    pub fn from_infohash(info_hash: InfoHash) -> Self {
        Self::from_infohash_with_params(
            info_hash,
            defaults::WINDOW_SECS,
            defaults::SLOT_MS,
            now_unix_secs(),
        )
    }

    /// Derive SRT from infohash with custom parameters.
    pub fn from_infohash_with_params(
        info_hash: InfoHash,
        window_secs: u64,
        slot_ms: u64,
        creation_time: u64,
    ) -> Self {
        let space_id = derive_space_id(&info_hash);
        let seed = derive_seed(&info_hash, creation_time);

        let token = SemanticRendezvousToken::new(
            space_id,
            seed,
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0: creation_time,
                window_secs,
                slot_ms,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::Simple,
        );

        Self {
            token,
            info_hash,
            creation_time,
        }
    }

    /// Create SwarmSrt from a TorrentMeta with SRT extension.
    pub fn from_torrent_meta(meta: &TorrentMeta) -> Result<Self, TorrentError> {
        let ext = meta
            .srt_extension
            .as_ref()
            .ok_or(TorrentError::InvalidTorrent("no SRT extension"))?;

        let creation_time = meta.creation_date.unwrap_or_else(now_unix_secs);
        let t0 = creation_time.saturating_add(ext.t0_offset);

        let space_id = derive_space_id(&meta.info_hash);
        let seed = if let Some(salt) = ext.salt {
            derive_seed_with_salt(&meta.info_hash, t0, &salt)
        } else {
            derive_seed(&meta.info_hash, t0)
        };

        let token = SemanticRendezvousToken::new(
            space_id,
            seed,
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0,
                window_secs: ext.window_secs,
                slot_ms: ext.slot_ms,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::Simple,
        );

        Ok(Self {
            token,
            info_hash: meta.info_hash,
            creation_time: t0,
        })
    }

    /// Parse SwarmSrt from a riftd-srt:// URI and infohash.
    ///
    /// Note: The infohash must be provided separately as it cannot be
    /// derived from the URI alone.
    pub fn from_uri(uri: &str, info_hash: InfoHash) -> Result<Self, TorrentError> {
        let token = SemanticRendezvousToken::from_uri(uri)
            .map_err(|e| TorrentError::Srt(e.to_string()))?;

        Ok(Self {
            info_hash,
            creation_time: token.time_model.t0,
            token,
        })
    }

    /// Encode as riftd-srt:// URI.
    pub fn to_uri(&self) -> Result<String, TorrentError> {
        self.token.to_uri().map_err(|e| TorrentError::Srt(e.to_string()))
    }

    /// Generate the SrtExtension struct for embedding in .torrent.
    pub fn to_extension(&self) -> SrtExtension {
        SrtExtension {
            version: 1,
            t0_offset: 0,
            window_secs: self.token.time_model.window_secs,
            slot_ms: self.token.time_model.slot_ms,
            salt: None,
        }
    }

    /// Get the rendezvous space ID.
    pub fn space_id(&self) -> &RendezvousSpaceId {
        &self.token.space
    }

    /// Get the time model.
    pub fn time_model(&self) -> &TimeModel {
        &self.token.time_model
    }

    /// Check if the SRT is currently valid (within time window).
    pub fn is_valid(&self) -> bool {
        let now = now_unix_secs();
        let t0 = self.token.time_model.t0;
        let end = t0.saturating_add(self.token.time_model.window_secs);
        now >= t0 && now < end
    }

    /// Get remaining time in the current window (seconds).
    pub fn remaining_secs(&self) -> u64 {
        let now = now_unix_secs();
        let end = self.token.time_model.t0.saturating_add(self.token.time_model.window_secs);
        end.saturating_sub(now)
    }

    /// Create a new SwarmSrt with a refreshed time window.
    pub fn refresh(&self) -> Self {
        Self::from_infohash_with_params(
            self.info_hash,
            self.token.time_model.window_secs,
            self.token.time_model.slot_ms,
            now_unix_secs(),
        )
    }
}

/// Derive RendezvousSpaceId from infohash (namespace isolation).
pub fn derive_space_id(info_hash: &InfoHash) -> RendezvousSpaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rift-torrent-space-v1");
    hasher.update(&info_hash.to_bytes32());
    let hash = hasher.finalize();
    RendezvousSpaceId(*hash.as_bytes())
}

/// Derive SRT seed from infohash + time anchor.
pub fn derive_seed(info_hash: &InfoHash, t0: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rift-torrent-seed-v1");
    hasher.update(&info_hash.to_bytes32());
    hasher.update(&t0.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Derive SRT seed with optional salt.
pub fn derive_seed_with_salt(info_hash: &InfoHash, t0: u64, salt: &[u8; 16]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rift-torrent-seed-v1");
    hasher.update(&info_hash.to_bytes32());
    hasher.update(&t0.to_le_bytes());
    hasher.update(salt);
    *hasher.finalize().as_bytes()
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_space_deterministic() {
        let hash = InfoHash::Sha1([0xAB; 20]);
        let s1 = derive_space_id(&hash);
        let s2 = derive_space_id(&hash);
        assert_eq!(s1.0, s2.0);
    }

    #[test]
    fn derive_seed_deterministic() {
        let hash = InfoHash::Sha1([0xCD; 20]);
        let t0 = 1700000000;
        let s1 = derive_seed(&hash, t0);
        let s2 = derive_seed(&hash, t0);
        assert_eq!(s1, s2);
    }

    #[test]
    fn different_hashes_different_seeds() {
        let h1 = InfoHash::Sha1([0x01; 20]);
        let h2 = InfoHash::Sha1([0x02; 20]);
        let t0 = 1700000000;
        assert_ne!(derive_seed(&h1, t0), derive_seed(&h2, t0));
    }

    #[test]
    fn different_times_different_seeds() {
        let hash = InfoHash::Sha1([0xEF; 20]);
        let s1 = derive_seed(&hash, 1700000000);
        let s2 = derive_seed(&hash, 1700000001);
        assert_ne!(s1, s2);
    }

    #[test]
    fn different_hashes_different_spaces() {
        let h1 = InfoHash::Sha1([0x01; 20]);
        let h2 = InfoHash::Sha1([0x02; 20]);
        assert_ne!(derive_space_id(&h1).0, derive_space_id(&h2).0);
    }

    #[test]
    fn swarm_srt_from_infohash() {
        let hash = InfoHash::Sha1([0xAB; 20]);
        let srt = SwarmSrt::from_infohash(hash);
        assert_eq!(srt.info_hash, hash);
        assert!(srt.is_valid());
    }

    #[test]
    fn srt_uri_roundtrip() {
        let hash = InfoHash::Sha1([0xCD; 20]);
        let srt = SwarmSrt::from_infohash(hash);
        let uri = srt.to_uri().unwrap();
        assert!(uri.starts_with("riftd-srt://"));

        let parsed = SwarmSrt::from_uri(&uri, hash).unwrap();
        assert_eq!(parsed.info_hash, hash);
    }

    #[test]
    fn salt_changes_seed() {
        let hash = InfoHash::Sha1([0xEF; 20]);
        let t0 = 1700000000;
        let salt = [0x42; 16];

        let s1 = derive_seed(&hash, t0);
        let s2 = derive_seed_with_salt(&hash, t0, &salt);
        assert_ne!(s1, s2);
    }
}
