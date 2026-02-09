//! Metrics and observations for Predictive Rendezvous.

/// NAT behavior observations (lightweight heuristic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatBehaviorHint {
    Unknown,
    PortPreserving,
    Patterned,
    HighVariance,
}

impl Default for NatBehaviorHint {
    fn default() -> Self {
        NatBehaviorHint::Unknown
    }
}

/// Per-rendezvous metrics captured during a run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
}

impl RendezvousMetrics {
    /// Create a zeroed metrics snapshot.
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}
