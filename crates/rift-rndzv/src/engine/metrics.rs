//! Metrics and observations for Predictive Rendezvous.

use crate::config::HybridMode;

/// NAT behavior observations (lightweight heuristic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum NatBehaviorHint {
    #[default]
    Unknown,
    PortPreserving,
    Patterned,
    HighVariance,
}


/// Which path won in a hybrid rendezvous attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridPath {
    Rndzv,
    Relay,
}

/// Per-rendezvous metrics captured during a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendezvousMetrics {
    pub slots_attempted: u64,
    pub slot_index_success: Option<u64>,
    pub total_duration_ms: u64,
    pub probes_sent: u64,
    pub probes_received: u64,
    pub escalations_triggered: u64,
    pub fallback_used: bool,
    pub fallback_method: Option<&'static str>,
    pub nat_behavior_hint: NatBehaviorHint,
    pub hybrid_mode: HybridMode,
    pub hints_used: bool,
    pub hybrid_winner: Option<HybridPath>,
}

impl RendezvousMetrics {
    /// Create a zeroed metrics snapshot.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for RendezvousMetrics {
    fn default() -> Self {
        Self {
            slots_attempted: 0,
            slot_index_success: None,
            total_duration_ms: 0,
            probes_sent: 0,
            probes_received: 0,
            escalations_triggered: 0,
            fallback_used: false,
            fallback_method: None,
            nat_behavior_hint: NatBehaviorHint::Unknown,
            hybrid_mode: HybridMode::default(),
            hints_used: false,
            hybrid_winner: None,
        }
    }
}
