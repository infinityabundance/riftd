//! Probe parsing and rendezvous state tracking.

use crate::{IdentityConstraints, Role, SemanticRendezvousToken, SlotParams};
use std::fmt;
use std::net::SocketAddr;

const PROBE_MAGIC: [u8; 4] = *b"PRV1";
const PROBE_LEN: usize = 4 + 8 + 8 + 16;

/// Parsed probe payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProbe {
    /// Deterministic rendezvous identifier.
    pub rendezvous_id: u64,
    /// Slot index for the probe.
    pub slot_index: u64,
    /// Truncated sender fingerprint.
    pub sender_fingerprint: [u8; 16],
}

/// Errors produced while parsing a probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    InvalidLength,
    InvalidMagic,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::InvalidLength => write!(f, "invalid probe length"),
            ProbeError::InvalidMagic => write!(f, "invalid probe magic"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Parse a raw probe payload into structured fields.
pub fn parse_probe_payload(bytes: &[u8]) -> Result<ParsedProbe, ProbeError> {
    if bytes.len() != PROBE_LEN {
        return Err(ProbeError::InvalidLength);
    }
    if bytes[0..4] != PROBE_MAGIC {
        return Err(ProbeError::InvalidMagic);
    }

    let rendezvous_id = u64::from_le_bytes([
        bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
    ]);
    let slot_index = u64::from_le_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
    ]);
    let mut sender_fingerprint = [0u8; 16];
    sender_fingerprint.copy_from_slice(&bytes[20..36]);

    Ok(ParsedProbe {
        rendezvous_id,
        slot_index,
        sender_fingerprint,
    })
}

/// Validate whether a parsed probe belongs to the provided SRT.
///
/// This checks the rendezvous identifier and applies identity constraints.
pub fn validate_probe_for_token(token: &SemanticRendezvousToken, parsed: &ParsedProbe) -> bool {
    if parsed.rendezvous_id != rendezvous_id_from_seed(&token.seed) {
        return false;
    }
    identity_allows(&token.identities, &parsed.sender_fingerprint)
}

fn identity_allows(identities: &IdentityConstraints, sender: &[u8; 16]) -> bool {
    if identities.allowed_fingerprints.is_empty() {
        return true;
    }
    identities
        .allowed_fingerprints
        .iter()
        .any(|fp| fp[..16] == sender[..])
}

/// Derive the rendezvous identifier from a seed.
pub fn rendezvous_id_from_seed(seed: &[u8; 32]) -> u64 {
    let digest = blake3::hash(seed);
    u64::from_le_bytes([
        digest.as_bytes()[0],
        digest.as_bytes()[1],
        digest.as_bytes()[2],
        digest.as_bytes()[3],
        digest.as_bytes()[4],
        digest.as_bytes()[5],
        digest.as_bytes()[6],
        digest.as_bytes()[7],
    ])
}

/// High-level rendezvous progress state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendezvousOutcome {
    InProgress,
    Succeeded {
        remote_addr: SocketAddr,
        slot_index: u64,
    },
    Failed,
}

/// Book-keeping for rendezvous progress.
pub struct RendezvousState {
    token: SemanticRendezvousToken,
    role: Role,
    last_sent_slot: Option<u64>,
    outcome: RendezvousOutcome,
}

impl RendezvousState {
    /// Create a new rendezvous state tracker.
    pub fn new(token: SemanticRendezvousToken, role: Role) -> Self {
        Self {
            token,
            role,
            last_sent_slot: None,
            outcome: RendezvousOutcome::InProgress,
        }
    }

    /// Record the last slot that was emitted.
    pub fn record_sent_slot(&mut self, slot: &SlotParams) {
        self.last_sent_slot = Some(slot.slot_index);
    }

    /// Record a received probe and update rendezvous outcome.
    pub fn record_received_probe(
        &mut self,
        addr: SocketAddr,
        probe: &ParsedProbe,
    ) -> RendezvousOutcome {
        match self.outcome {
            RendezvousOutcome::Succeeded { .. } | RendezvousOutcome::Failed => {
                return self.outcome.clone();
            }
            RendezvousOutcome::InProgress => {}
        }

        if validate_probe_for_token(&self.token, probe) {
            self.outcome = RendezvousOutcome::Succeeded {
                remote_addr: addr,
                slot_index: probe.slot_index,
            };
        }

        self.outcome.clone()
    }

    /// Current outcome without mutation.
    pub fn outcome(&self) -> RendezvousOutcome {
        self.outcome.clone()
    }

    /// Role used for schedule derivation.
    pub fn role(&self) -> Role {
        self.role
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_probe_payload, EscalationPolicy, ProbePayload, SearchStrategy, TimeModel};

    fn token_with_seed(seed: [u8; 32]) -> SemanticRendezvousToken {
        SemanticRendezvousToken::new(
            crate::api::RendezvousSpaceId([0u8; 32]),
            seed,
            IdentityConstraints {
                allowed_fingerprints: vec![[0xAAu8; 32]],
            },
            TimeModel {
                t0: 1_700_000_000,
                window_secs: 10,
                slot_ms: 250,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        )
    }

    #[test]
    fn parse_probe_round_trip() {
        let seed = [1u8; 32];
        let payload = ProbePayload {
            rendezvous_id: rendezvous_id_from_seed(&seed),
            slot_index: 42,
            sender_fingerprint: [0xAA; 16],
        };
        let bytes = build_probe_payload(payload);
        let parsed = parse_probe_payload(&bytes).expect("parse");

        assert_eq!(parsed.rendezvous_id, payload.rendezvous_id);
        assert_eq!(parsed.slot_index, payload.slot_index);
        assert_eq!(parsed.sender_fingerprint, payload.sender_fingerprint);
    }

    #[test]
    fn parse_probe_rejects_bad_magic() {
        let mut bytes = vec![0u8; PROBE_LEN];
        bytes[0..4].copy_from_slice(b"NOPE");
        let err = parse_probe_payload(&bytes).expect_err("bad magic");
        assert_eq!(err, ProbeError::InvalidMagic);
    }

    #[test]
    fn validate_probe_matches_token() {
        let token = token_with_seed([2u8; 32]);
        let probe = ParsedProbe {
            rendezvous_id: rendezvous_id_from_seed(&token.seed),
            slot_index: 7,
            sender_fingerprint: [0xAA; 16],
        };

        assert!(validate_probe_for_token(&token, &probe));
    }

    #[test]
    fn validate_probe_rejects_unknown_sender() {
        let token = token_with_seed([3u8; 32]);
        let probe = ParsedProbe {
            rendezvous_id: rendezvous_id_from_seed(&token.seed),
            slot_index: 7,
            sender_fingerprint: [0xBB; 16],
        };

        assert!(!validate_probe_for_token(&token, &probe));
    }

    #[test]
    fn state_transitions_to_success() {
        let token = token_with_seed([4u8; 32]);
        let mut state = RendezvousState::new(token, Role::Caller);
        let addr: SocketAddr = "127.0.0.1:7000".parse().unwrap();
        let probe = ParsedProbe {
            rendezvous_id: rendezvous_id_from_seed(&state.token.seed),
            slot_index: 5,
            sender_fingerprint: [0xAA; 16],
        };

        let outcome = state.record_received_probe(addr, &probe);
        assert!(matches!(outcome, RendezvousOutcome::Succeeded { .. }));
        assert!(matches!(state.outcome(), RendezvousOutcome::Succeeded { .. }));
    }
}
