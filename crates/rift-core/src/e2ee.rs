//! End-to-end encryption helpers.
//!
//! This module provides:
//! - X25519 keypair generation for session encryption
//! - Ed25519 signatures over ephemeral keys
//! - HKDF-based shared key derivation

use ed25519_dalek::{PublicKey as Ed25519PublicKey, Signature, Signer, Verifier};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::Identity;

pub struct E2eeKeypair {
    /// X25519 private key.
    pub secret: StaticSecret,
    /// X25519 public key.
    pub public: X25519PublicKey,
}

/// Generate an ephemeral X25519 keypair for E2EE.
pub fn generate_e2ee_keypair() -> E2eeKeypair {
    let secret = StaticSecret::new(OsRng);
    let public = X25519PublicKey::from(&secret);
    E2eeKeypair { secret, public }
}

/// Sign the ephemeral public key with the long-term Ed25519 identity.
pub fn sign_e2ee_public(identity: &Identity, session_id: &[u8], public: &X25519PublicKey) -> Vec<u8> {
    let msg = e2ee_message(session_id, public);
    let signature: Signature = identity.keypair.sign(&msg);
    signature.to_bytes().to_vec()
}

/// Verify a peer's signature over their ephemeral public key.
pub fn verify_e2ee_public(
    peer_public: &Ed25519PublicKey,
    session_id: &[u8],
    public: &X25519PublicKey,
    signature: &[u8],
) -> bool {
    let Ok(signature) = Signature::from_bytes(signature) else {
        return false;
    };
    let msg = e2ee_message(session_id, public);
    peer_public.verify(&msg, &signature).is_ok()
}

/// Derive the shared session key from X25519 DH + HKDF.
pub fn derive_e2ee_shared_key(
    secret: &StaticSecret,
    remote_public: &X25519PublicKey,
    session_id: &[u8],
) -> [u8; 32] {
    let shared = secret.diffie_hellman(remote_public);
    let hk = Hkdf::<Sha256>::new(Some(session_id), shared.as_bytes());
    let mut out = [0u8; 32];
    hk.expand(b"rift-e2ee-v1", &mut out)
        .expect("hkdf expand");
    out
}

/// Compose the message used for signature verification.
fn e2ee_message(session_id: &[u8], public: &X25519PublicKey) -> Vec<u8> {
    let mut msg = Vec::with_capacity(16 + session_id.len() + 32);
    msg.extend_from_slice(b"rift-e2ee");
    msg.extend_from_slice(session_id);
    msg.extend_from_slice(public.as_bytes());
    msg
}

/// Helper to convert raw bytes into an X25519 public key.
pub fn public_key_from_bytes(bytes: [u8; 32]) -> X25519PublicKey {
    X25519PublicKey::from(bytes)
}

/// Helper to convert raw bytes into an Ed25519 public key.
pub fn ed25519_public_from_bytes(bytes: &[u8]) -> Option<Ed25519PublicKey> {
    Ed25519PublicKey::from_bytes(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e2ee_sign_verify_roundtrip() {
        let identity = Identity::generate();
        let session = [7u8; 32];
        let kp = generate_e2ee_keypair();
        let sig = sign_e2ee_public(&identity, &session, &kp.public);
        let ok = verify_e2ee_public(&identity.keypair.public, &session, &kp.public, &sig);
        assert!(ok);
    }

    #[test]
    fn e2ee_shared_key_matches() {
        let session = [9u8; 32];
        let a = generate_e2ee_keypair();
        let b = generate_e2ee_keypair();
        let ka = derive_e2ee_shared_key(&a.secret, &b.public, &session);
        let kb = derive_e2ee_shared_key(&b.secret, &a.public, &session);
        assert_eq!(ka, kb);
    }

    #[test]
    fn e2ee_bad_signature_rejected() {
        let identity = Identity::generate();
        let other = Identity::generate();
        let session = [1u8; 32];
        let kp = generate_e2ee_keypair();
        let sig = sign_e2ee_public(&identity, &session, &kp.public);
        let ok = verify_e2ee_public(&other.keypair.public, &session, &kp.public, &sig);
        assert!(!ok);
    }
}
