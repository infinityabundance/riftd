/// Constraints on acceptable peer identities.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IdentityConstraints {
    /// Allowed peer fingerprints (32-byte hashes or public key digests).
    pub allowed_fingerprints: Vec<[u8; 32]>,
}
