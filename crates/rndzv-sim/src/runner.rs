use std::net::SocketAddr;

use rift_rndzv::{
    build_probe_payload, compute_slot_params, rendezvous_id_from_seed, validate_probe_for_token,
    EscalationPolicy, IdentityConstraints, RendezvousMetrics, Role, SearchStrategy, SemanticRendezvousToken,
    TimeModel,
};

#[derive(Debug)]
pub struct SimRunner {
    token: SemanticRendezvousToken,
    rendezvous_id: u64,
    metrics: RendezvousMetrics,
    done: bool,
    pending_response: bool,
    last_sender: Option<SocketAddr>,
    target_ports: Vec<u16>,
}

impl SimRunner {
    pub fn new(token: SemanticRendezvousToken) -> Self {
        let rendezvous_id = rendezvous_id_from_seed(&token.seed);
        let target_ports = (0..1000u16).map(|i| 40000u16.wrapping_add(i)).collect();
        Self {
            token,
            rendezvous_id,
            metrics: RendezvousMetrics::new(),
            done: false,
            pending_response: false,
            last_sender: None,
            target_ports,
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<(SocketAddr, Vec<u8>)> {
        if self.done {
            return Vec::new();
        }
        let slot = compute_slot_params(&self.token.seed, &self.token.time_model, Role::Symmetric, now_ms);
        let mut out = Vec::new();
        if let Some(slot) = slot {
            self.metrics.slots_attempted = self.metrics.slots_attempted.max(slot.slot_index + 1);
            let payload = build_probe_payload(rift_rndzv::ProbePayload {
                rendezvous_id: self.rendezvous_id,
                slot_index: slot.slot_index,
                sender_fingerprint: [0u8; 16],
            });
            for port in &self.target_ports {
                let addr = SocketAddr::from(([203, 0, 113, 99], *port));
                out.push((addr, payload.clone()));
            }
            self.metrics.probes_sent += 1;
        }
        if self.pending_response {
            let payload = build_probe_payload(rift_rndzv::ProbePayload {
                rendezvous_id: self.rendezvous_id,
                slot_index: self.metrics.slot_index_success.unwrap_or(0),
                sender_fingerprint: [0u8; 16],
            });
            if let Some(addr) = self.last_sender {
                out.push((addr, payload));
                self.metrics.probes_sent += 1;
            }
            self.pending_response = false;
        }
        out
    }

    pub fn on_probe(&mut self, src: SocketAddr, payload: &[u8]) {
        if self.done {
            return;
        }
        if let Ok(parsed) = rift_rndzv::parse_probe_payload(payload) {
            self.metrics.probes_received += 1;
            if validate_probe_for_token(&self.token, &parsed) {
                self.metrics.slot_index_success = Some(parsed.slot_index);
                self.last_sender = Some(src);
                self.pending_response = true;
                self.done = true;
            }
        }
    }

    pub fn is_complete(&self) -> bool {
        self.done
    }

    pub fn metrics(&self) -> &RendezvousMetrics {
        &self.metrics
    }
}

impl Default for SimRunner {
    fn default() -> Self {
        Self::new(SemanticRendezvousToken::new(
            rift_rndzv::RendezvousSpaceId([0u8; 32]),
            [0u8; 32],
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0: 0,
                window_secs: 1,
                slot_ms: 100,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        ))
    }
}
