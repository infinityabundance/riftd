use std::collections::HashMap;

use crate::peer::{SimPeer, SimPeerId};

#[derive(Debug, Default, Clone)]
pub struct SimOutcome {
    pub success: bool,
    pub ticks: u64,
    pub slots_attempted: u64,
}

pub struct SimNetwork {
    now_ms: u64,
    peers: HashMap<SimPeerId, SimPeer>,
}

impl SimNetwork {
    pub fn new(start_ms: u64) -> Self {
        Self {
            now_ms: start_ms,
            peers: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, peer: SimPeer) {
        self.peers.insert(peer.id, peer);
    }

    pub fn step(&mut self, tick_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(tick_ms);
        let mut outbound = Vec::new();

        for peer in self.peers.values_mut() {
            let sends = peer.runner.tick(self.now_ms);
            for (dest, payload) in sends {
                let external_src = if let Some(nat) = peer.nat.as_mut() {
                    nat.translate_outbound(peer.local_addr, dest, self.now_ms)
                } else {
                    peer.public_addr
                };
                outbound.push((external_src, dest, payload));
            }
        }

        // Deliver outbound through NAT mappings.
        let mut deliveries = Vec::new();
        for (src, dst, payload) in outbound {
            for peer in self.peers.values_mut() {
                let internal = if let Some(nat) = peer.nat.as_mut() {
                    nat.translate_inbound(dst, self.now_ms)
                } else if peer.public_addr == dst {
                    Some(peer.local_addr)
                } else {
                    None
                };
                if internal.is_some() {
                    deliveries.push((peer.id, src, payload.clone()));
                }
            }
        }
        for (peer_id, src, payload) in deliveries {
            if let Some(peer) = self.peers.get_mut(&peer_id) {
                peer.inbox.push((self.now_ms, src, payload));
            }
        }

        // Deliver inbound to runners.
        for peer in self.peers.values_mut() {
            let inbox = std::mem::take(&mut peer.inbox);
            for (_ts, src, payload) in inbox {
                peer.runner.on_probe(src, &payload);
            }
        }
    }

    pub fn run_until(&mut self, deadline_ms: u64) -> SimOutcome {
        let mut outcome = SimOutcome::default();
        while self.now_ms < deadline_ms {
            self.step(50);
            outcome.ticks += 1;
            outcome.slots_attempted = self
                .peers
                .values()
                .map(|p| p.runner.metrics().slots_attempted)
                .max()
                .unwrap_or(0);
            if self.peers.values().all(|p| p.runner.is_complete()) {
                outcome.success = true;
                break;
            }
        }
        outcome
    }
}
