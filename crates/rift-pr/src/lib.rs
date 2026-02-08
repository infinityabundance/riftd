//! Predictive Rendezvous (PR) core types.

pub mod identity;
pub mod strategy;
pub mod time;

pub use identity::IdentityConstraints;
pub use strategy::{EscalationPolicy, SearchStrategy};
pub use time::TimeModel;

/// Semantic Rendezvous Token (SRT) describes the deterministic rendezvous schedule
/// and the constraints used to locate peers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticRendezvousToken {
    /// 32-byte seed that deterministically drives schedule generation.
    pub seed: [u8; 32],
    /// Identity constraints that limit acceptable peers.
    pub identities: IdentityConstraints,
    /// Time model used for schedule slotting and windows.
    pub time_model: TimeModel,
    /// Search strategy used to traverse the deterministic schedule.
    pub search_strategy: SearchStrategy,
    /// Escalation policy describing how the search evolves over time.
    pub escalation: EscalationPolicy,
}

impl SemanticRendezvousToken {
    /// Construct a new SRT from its component parts.
    pub fn new(
        seed: [u8; 32],
        identities: IdentityConstraints,
        time_model: TimeModel,
        search_strategy: SearchStrategy,
        escalation: EscalationPolicy,
    ) -> Self {
        Self {
            seed,
            identities,
            time_model,
            search_strategy,
            escalation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_srt() {
        let seed = [7u8; 32];
        let identities = IdentityConstraints {
            allowed_fingerprints: vec![[1u8; 32], [2u8; 32]],
        };
        let time_model = TimeModel {
            t0: 1_700_000_000,
            window_secs: 120,
            slot_ms: 250,
        };
        let search_strategy = SearchStrategy::BasicDeterministic;
        let escalation = EscalationPolicy::None;

        let token = SemanticRendezvousToken::new(
            seed,
            identities.clone(),
            time_model.clone(),
            search_strategy,
            escalation,
        );

        assert_eq!(token.seed, seed);
        assert_eq!(token.identities, identities);
        assert_eq!(token.time_model, time_model);
        assert_eq!(token.search_strategy, search_strategy);
        assert_eq!(token.escalation, escalation);
    }
}
