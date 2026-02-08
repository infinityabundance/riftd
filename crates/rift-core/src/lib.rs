//! Core types and helpers that are shared across the Rift stack.
//!
//! This crate is intentionally small and dependency-light so it can be reused by
//! native clients, SDKs, and (now) WASM targets. It provides:
//! - identity/key handling
//! - invite encoding/decoding
//! - message and channel identifiers
//! - Noise/E2EE key utilities

/// Error types used by the core crate.
pub mod error;
/// Identity generation and persistence helpers.
pub mod identity;
/// On-disk keystore helpers for identity rotation and discovery.
pub mod keystore;
/// Invite creation and parsing helpers.
pub mod invite;
/// Peer/channel/message ID helpers.
pub mod message;
/// Noise protocol helpers (session setup).
pub mod noise;
/// E2EE helper functions (X25519 + signatures + HKDF).
pub mod e2ee;

// Re-exports keep core consumers from needing to know the internal module layout.
pub use error::CoreError;
pub use identity::Identity;
pub use identity::peer_id_from_public_key_bytes;
pub use keystore::{KeyStore, KeyStoreError};
pub use invite::{decode_invite, encode_invite, generate_invite, Invite};
pub use message::{ChannelId, MessageId, PeerId};
