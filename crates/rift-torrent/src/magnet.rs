//! Magnet URI parsing with SRT extension support.

use crate::{InfoHash, TorrentError};
use std::net::SocketAddr;

/// Parsed magnet URI.
#[derive(Clone, Debug)]
pub struct MagnetUri {
    /// Infohash(es) from xt parameter.
    pub info_hashes: Vec<InfoHash>,
    /// Display name (dn parameter).
    pub display_name: Option<String>,
    /// Tracker URLs (tr parameters).
    pub trackers: Vec<String>,
    /// Web seed URLs (ws parameters).
    pub web_seeds: Vec<String>,
    /// SRT URI (xs parameter with riftd-srt:// scheme).
    pub srt_uri: Option<String>,
    /// Peer addresses (x.pe parameter).
    pub peers: Vec<SocketAddr>,
}

/// Standard magnet prefix.
const MAGNET_PREFIX: &str = "magnet:?";

impl MagnetUri {
    /// Parse a magnet URI string.
    pub fn parse(uri: &str) -> Result<Self, TorrentError> {
        if !uri.starts_with(MAGNET_PREFIX) {
            return Err(TorrentError::InvalidMagnet("missing magnet:? prefix"));
        }

        let query = &uri[MAGNET_PREFIX.len()..];
        let mut info_hashes = Vec::new();
        let mut display_name = None;
        let mut trackers = Vec::new();
        let mut web_seeds = Vec::new();
        let mut srt_uri = None;
        let mut peers = Vec::new();

        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }

            let (key, value) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => continue,
            };

            let value = urlencoding::decode(value)
                .map_err(|e| TorrentError::UrlEncoding(e.to_string()))?;

            match key {
                "xt" => {
                    // urn:btih:<hash> for SHA1, urn:btmh:<hash> for SHA256
                    if let Some(hash) = value.strip_prefix("urn:btih:") {
                        if let Ok(h) = InfoHash::from_hex(hash) {
                            info_hashes.push(h);
                        }
                    } else if let Some(hash) = value.strip_prefix("urn:btmh:") {
                        if let Ok(h) = InfoHash::from_hex(hash) {
                            info_hashes.push(h);
                        }
                    }
                }
                "dn" => display_name = Some(value.into_owned()),
                "tr" => trackers.push(value.into_owned()),
                "ws" => web_seeds.push(value.into_owned()),
                "xs" => {
                    // Exact source - check for SRT URI
                    if value.starts_with("riftd-srt://") {
                        srt_uri = Some(value.into_owned());
                    }
                }
                "x.pe" => {
                    // Peer address
                    if let Ok(addr) = value.parse() {
                        peers.push(addr);
                    }
                }
                _ => {} // Ignore unknown parameters
            }
        }

        if info_hashes.is_empty() {
            return Err(TorrentError::InvalidMagnet("no infohash found"));
        }

        Ok(MagnetUri {
            info_hashes,
            display_name,
            trackers,
            web_seeds,
            srt_uri,
            peers,
        })
    }

    /// Generate a magnet URI string.
    pub fn to_uri(&self) -> String {
        let mut parts = Vec::new();

        for hash in &self.info_hashes {
            let prefix = match hash {
                InfoHash::Sha1(_) => "urn:btih:",
                InfoHash::Sha256(_) => "urn:btmh:",
            };
            parts.push(format!("xt={}{}", prefix, hash.to_hex()));
        }

        if let Some(dn) = &self.display_name {
            parts.push(format!("dn={}", urlencoding::encode(dn)));
        }

        for tr in &self.trackers {
            parts.push(format!("tr={}", urlencoding::encode(tr)));
        }

        for ws in &self.web_seeds {
            parts.push(format!("ws={}", urlencoding::encode(ws)));
        }

        // Add SRT URI in xs parameter
        if let Some(srt) = &self.srt_uri {
            parts.push(format!("xs={}", urlencoding::encode(srt)));
        }

        for peer in &self.peers {
            parts.push(format!("x.pe={}", peer));
        }

        format!("magnet:?{}", parts.join("&"))
    }

    /// Check if this magnet has SRT extension.
    pub fn has_srt(&self) -> bool {
        self.srt_uri.is_some()
    }

    /// Get primary infohash.
    pub fn primary_hash(&self) -> &InfoHash {
        &self.info_hashes[0]
    }

    /// Create a new magnet URI with just an infohash.
    pub fn from_infohash(info_hash: InfoHash) -> Self {
        Self {
            info_hashes: vec![info_hash],
            display_name: None,
            trackers: Vec::new(),
            web_seeds: Vec::new(),
            srt_uri: None,
            peers: Vec::new(),
        }
    }

    /// Add a display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Add an SRT URI.
    pub fn with_srt(mut self, srt_uri: impl Into<String>) -> Self {
        self.srt_uri = Some(srt_uri.into());
        self
    }

    /// Add a tracker.
    pub fn with_tracker(mut self, tracker: impl Into<String>) -> Self {
        self.trackers.push(tracker.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_magnet() {
        let uri = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Test";
        let magnet = MagnetUri::parse(uri).unwrap();
        assert_eq!(magnet.display_name, Some("Test".to_string()));
        assert!(!magnet.has_srt());
    }

    #[test]
    fn parse_magnet_with_srt() {
        let uri = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&xs=riftd-srt://v1?foo=bar";
        let magnet = MagnetUri::parse(uri).unwrap();
        assert!(magnet.has_srt());
        assert_eq!(magnet.srt_uri, Some("riftd-srt://v1?foo=bar".to_string()));
    }

    #[test]
    fn magnet_roundtrip() {
        let original = MagnetUri::from_infohash(InfoHash::Sha1([0xAB; 20]))
            .with_name("Test File")
            .with_tracker("http://tracker.example.com/announce")
            .with_srt("riftd-srt://v1?space=abc");

        let uri = original.to_uri();
        let parsed = MagnetUri::parse(&uri).unwrap();

        assert_eq!(parsed.display_name, original.display_name);
        assert_eq!(parsed.trackers, original.trackers);
        assert_eq!(parsed.srt_uri, original.srt_uri);
    }

    #[test]
    fn reject_invalid_magnet() {
        assert!(MagnetUri::parse("http://example.com").is_err());
        assert!(MagnetUri::parse("magnet:?dn=NoHash").is_err());
    }

    #[test]
    fn parse_multiple_trackers() {
        let uri = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&tr=http://a.com&tr=http://b.com";
        let magnet = MagnetUri::parse(uri).unwrap();
        assert_eq!(magnet.trackers.len(), 2);
    }
}
