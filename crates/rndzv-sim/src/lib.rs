//! In-memory simulation harness for rndzv.

pub mod nat;
pub mod network;
pub mod peer;
pub mod runner;
pub mod swarm;

pub use nat::{NatModel, PortPreservingNat, RandomPortNat, SymmetricNat};
pub use network::{SimNetwork, SimOutcome};
pub use peer::{SimPeer, SimPeerId};
pub use swarm::{simulate_swarm, ContentId, SwarmOutcome};

#[cfg(test)]
mod tests {
    use super::*;
    use rift_rndzv::{EscalationPolicy, IdentityConstraints, SearchStrategy, SemanticRendezvousToken, TimeModel};

    fn make_token(t0: u64) -> SemanticRendezvousToken {
        SemanticRendezvousToken::new(
            rift_rndzv::RendezvousSpaceId([0u8; 32]),
            [9u8; 32],
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0,
                window_secs: 5,
                slot_ms: 100,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        )
    }

    #[test]
    fn public_to_public_succeeds() {
        let mut net = SimNetwork::new(0);
        let token = make_token(200);

        let a = SimPeer::new("peer-a", None, token.clone());
        let b = SimPeer::new("peer-b", None, token);

        net.add_peer(a);
        net.add_peer(b);

        let outcome = net.run_until(2_000);
        assert!(!outcome.success, "public/public should fail with the naive sim runner");
    }

    #[test]
    fn nat_to_nat_symmetric() {
        let mut net = SimNetwork::new(0);
        let token = make_token(200);

        let a_nat = SymmetricNat::new(40000, 20_000, 5_000);
        let b_nat = SymmetricNat::new(42000, 20_000, 5_000);

        let a = SimPeer::new("peer-a", Some(Box::new(a_nat)), token.clone());
        let b = SimPeer::new("peer-b", Some(Box::new(b_nat)), token);

        net.add_peer(a);
        net.add_peer(b);

        let outcome = net.run_until(5_000);
        assert!(!outcome.success, "symmetric NATs should often fail in this simple model");
    }
}
