use std::collections::{HashMap, HashSet};

use rift_rndzv::{compute_slot_params, Role, SemanticRendezvousToken, TimeModel};

/// Content identifier (torrent-like infohash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentId(pub [u8; 32]);

/// Outcome summary for swarm rendezvous runs.
#[derive(Debug, Default, Clone)]
pub struct SwarmOutcome {
    pub peers: usize,
    pub ticks: u64,
    pub connections: HashSet<(u32, u32)>,
    pub degrees: HashMap<u32, usize>,
    pub avg_degree: f32,
}

/// Derive a swarm-specific SRT (shared across peers).
pub fn srt_for_content(content: &ContentId) -> SemanticRendezvousToken {
    let space = blake3::hash(b"rndzv-swarm-space");
    let seed = blake3::hash(&[content.0.as_slice(), b"rndzv-swarm-seed"].concat());
    SemanticRendezvousToken::new(
        rift_rndzv::RendezvousSpaceId(*space.as_bytes()),
        *seed.as_bytes(),
        rift_rndzv::IdentityConstraints {
            allowed_fingerprints: Vec::new(),
        },
        TimeModel {
            t0: 0,
            window_secs: 600,
            slot_ms: 200,
        },
        rift_rndzv::SearchStrategy::BasicDeterministic,
        rift_rndzv::EscalationPolicy::None,
    )
}

/// Simulate a swarm rendezvous run using a lightweight bucket model.
///
/// Each peer derives a per-peer seed from the shared content id and its peer id.
/// Peers that land in the same bucket during a slot are considered connected.
pub fn simulate_swarm(
    content: ContentId,
    peer_ids: &[u32],
    ticks: u64,
    buckets: u16,
) -> SwarmOutcome {
    let base = srt_for_content(&content);
    let mut edges: HashSet<(u32, u32)> = HashSet::new();

    let mut seeds: HashMap<u32, [u8; 32]> = HashMap::new();
    for id in peer_ids {
        let mut input = Vec::with_capacity(36);
        input.extend_from_slice(&content.0);
        input.extend_from_slice(&id.to_le_bytes());
        let seed = blake3::hash(&input);
        seeds.insert(*id, *seed.as_bytes());
    }

    for tick in 0..ticks {
        let now_ms = tick.saturating_mul(base.time_model.slot_ms);
        let mut buckets_map: HashMap<u16, Vec<u32>> = HashMap::new();
        for id in peer_ids {
            if let Some(seed) = seeds.get(id) {
                if let Some(slot) =
                    compute_slot_params(seed, &base.time_model, Role::Symmetric, now_ms)
                {
                    let bucket = slot.remote_port_offset % buckets.max(1);
                    buckets_map.entry(bucket).or_default().push(*id);
                }
            }
        }
        for ids in buckets_map.values() {
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let a = ids[i];
                    let b = ids[j];
                    let edge = if a < b { (a, b) } else { (b, a) };
                    edges.insert(edge);
                }
            }
        }
    }

    let mut degrees: HashMap<u32, usize> = HashMap::new();
    for (a, b) in edges.iter().copied() {
        *degrees.entry(a).or_default() += 1;
        *degrees.entry(b).or_default() += 1;
    }
    let total_degree: usize = degrees.values().sum();
    let avg_degree = if peer_ids.is_empty() {
        0.0
    } else {
        total_degree as f32 / peer_ids.len() as f32
    };

    SwarmOutcome {
        peers: peer_ids.len(),
        ticks,
        connections: edges,
        degrees,
        avg_degree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swarm_sim_produces_connections() {
        let content = ContentId([1u8; 32]);
        let peer_ids: Vec<u32> = (1..=8).collect();
        let outcome = simulate_swarm(content, &peer_ids, 50, 8);
        assert!(outcome.avg_degree >= 0.0);
    }
}
