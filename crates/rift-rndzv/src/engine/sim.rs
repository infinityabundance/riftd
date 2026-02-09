//! NAT and topology simulation harness for Predictive Rendezvous.

use crate::{compute_slot_params, Role, TimeModel};
use std::collections::HashMap;

/// Simple simulation address (IPv4-like integer + port).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimAddr {
    pub ip: u32,
    pub port: u16,
}

/// NAT behavior variants for simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatBehavior {
    /// External port mirrors the internal port.
    PortPreserving,
    /// External port is derived from a hash of (internal port, remote addr).
    SymmetricHashing,
    /// External port is pseudo-random per mapping.
    RandomAllocation,
}

/// Configuration for a NAT model.
#[derive(Debug, Clone, Copy)]
pub struct NatConfig {
    pub behavior: NatBehavior,
    /// Base port used for hash/random offset.
    pub base_port: u16,
    /// Mapping timeout in milliseconds.
    pub mapping_ttl_ms: u64,
}

#[derive(Debug, Clone)]
struct MappingEntry {
    external_port: u16,
    internal_port: u16,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MappingKey {
    internal_port: u16,
    remote_ip: u32,
    remote_port: u16,
}

/// Deterministic pseudo-random number generator (LCG) for simulations.
#[derive(Debug, Clone)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }
}

/// NAT mapping simulator.
#[derive(Debug, Clone)]
pub struct NatSim {
    pub config: NatConfig,
    rng: Lcg,
    mappings: HashMap<MappingKey, MappingEntry>,
    port_index: HashMap<u16, MappingEntry>,
}

impl NatSim {
    pub fn new(config: NatConfig, seed: u64) -> Self {
        Self {
            config,
            rng: Lcg::new(seed),
            mappings: HashMap::new(),
            port_index: HashMap::new(),
        }
    }

    pub fn map_outbound(&mut self, internal_port: u16, remote: SimAddr, now_ms: u64) -> u16 {
        self.evict_expired(now_ms);
        let key = MappingKey {
            internal_port,
            remote_ip: remote.ip,
            remote_port: remote.port,
        };
        if let Some(entry) = self.mappings.get(&key) {
            return entry.external_port;
        }

        let external_port = match self.config.behavior {
            NatBehavior::PortPreserving => internal_port,
            NatBehavior::SymmetricHashing => {
                let hash = hash_u64(
                    (internal_port as u64) << 48
                        | (remote.ip as u64) << 16
                        | remote.port as u64,
                );
                self.config.base_port.wrapping_add((hash as u16) & 0x7FFF)
            }
            NatBehavior::RandomAllocation => {
                let val = self.rng.next_u32() as u16;
                self.config.base_port.wrapping_add(val & 0x7FFF)
            }
        };

        let entry = MappingEntry {
            external_port,
            internal_port,
            expires_at_ms: now_ms.saturating_add(self.config.mapping_ttl_ms),
        };
        self.mappings.insert(key, entry.clone());
        self.port_index.insert(external_port, entry);
        external_port
    }

    pub fn allow_inbound(&mut self, external_port: u16, now_ms: u64) -> Option<u16> {
        self.evict_expired(now_ms);
        self.port_index.get(&external_port).map(|entry| entry.internal_port)
    }

    fn evict_expired(&mut self, now_ms: u64) {
        self.mappings.retain(|_, entry| entry.expires_at_ms > now_ms);
        self.port_index.retain(|_, entry| entry.expires_at_ms > now_ms);
    }
}

fn hash_u64(mut input: u64) -> u64 {
    // Simple reversible mix (not cryptographic; simulation-only).
    input ^= input >> 33;
    input = input.wrapping_mul(0xff51afd7ed558ccd);
    input ^= input >> 33;
    input = input.wrapping_mul(0xc4ceb9fe1a85ec53);
    input ^= input >> 33;
    input
}

/// Simulation parameters for a peer.
#[derive(Debug, Clone)]
pub struct SimPeer {
    pub seed: [u8; 32],
    pub role: Role,
    pub base_internal_port: u16,
    pub nat: NatSim,
    pub public_ip: u32,
}

#[derive(Debug, Clone)]
pub struct SimConfig {
    pub time_model: TimeModel,
    pub duration_ms: u64,
    pub slot_step_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SimEvent {
    pub time_ms: u64,
    pub slot_index: u64,
    pub sender: &'static str,
    pub receiver: &'static str,
    pub delivered: bool,
}

#[derive(Debug, Clone)]
pub struct SimReport {
    pub events: Vec<SimEvent>,
    pub slots_attempted: u64,
    pub success: bool,
    pub first_success_slot: Option<u64>,
}

/// Run a two-peer Predictive Rendezvous simulation.
pub fn run_simulation(
    cfg: SimConfig,
    peer_a: &mut SimPeer,
    peer_b: &mut SimPeer,
) -> SimReport {
    let mut events = Vec::new();
    let mut slots_attempted = 0;
    let mut success = false;
    let mut first_success_slot = None;

    let mut now_ms = cfg.time_model.t0.saturating_mul(1_000);
    let end_ms = now_ms.saturating_add(cfg.duration_ms);

    while now_ms < end_ms {
        let slot_a = compute_slot_params(&peer_a.seed, &cfg.time_model, peer_a.role, now_ms);
        let slot_b = compute_slot_params(&peer_b.seed, &cfg.time_model, peer_b.role, now_ms);

        if let (Some(slot_a), Some(slot_b)) = (slot_a, slot_b) {
            slots_attempted += 1;

            let a_remote = SimAddr {
                ip: peer_b.public_ip,
                port: peer_b.nat.config.base_port.wrapping_add(slot_a.remote_port_offset),
            };
            let b_remote = SimAddr {
                ip: peer_a.public_ip,
                port: peer_a.nat.config.base_port.wrapping_add(slot_b.remote_port_offset),
            };

            let a_external = peer_a
                .nat
                .map_outbound(peer_a.base_internal_port.wrapping_add(slot_a.local_port_offset), a_remote, now_ms);
            let b_external = peer_b
                .nat
                .map_outbound(peer_b.base_internal_port.wrapping_add(slot_b.local_port_offset), b_remote, now_ms);

            let a_delivered = peer_b.nat.allow_inbound(a_external, now_ms).is_some();
            let b_delivered = peer_a.nat.allow_inbound(b_external, now_ms).is_some();

            let delivered = a_delivered && b_delivered;
            if delivered && !success {
                success = true;
                first_success_slot = Some(slot_a.slot_index);
            }

            events.push(SimEvent {
                time_ms: now_ms,
                slot_index: slot_a.slot_index,
                sender: "A",
                receiver: "B",
                delivered: a_delivered,
            });
            events.push(SimEvent {
                time_ms: now_ms,
                slot_index: slot_b.slot_index,
                sender: "B",
                receiver: "A",
                delivered: b_delivered,
            });
        }

        now_ms = now_ms.saturating_add(cfg.slot_step_ms.max(1));
    }

    SimReport {
        events,
        slots_attempted,
        success,
        first_success_slot,
    }
}

/// Render a CSV summary of the simulation events.
pub fn render_csv(report: &SimReport) -> String {
    let mut out = String::from("time_ms,slot_index,sender,receiver,delivered\n");
    for event in &report.events {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            event.time_ms, event.slot_index, event.sender, event.receiver, event.delivered
        ));
    }
    out
}
