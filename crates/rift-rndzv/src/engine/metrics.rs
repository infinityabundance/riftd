//! Metrics and observations for Predictive Rendezvous.

/// Per-rendezvous metrics captured during a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendezvousMetrics {
    pub slots_attempted: u64,
    pub slots_succeeded: Option<u64>,
    pub total_duration_ms: u64,
    pub probes_sent: u64,
    pub probes_received: u64,
    pub escalations_triggered: u64,
    pub nat_behavior_notes: Vec<String>,
    pub fallback_used: bool,
    pub fallback_method: Option<String>,
    pub time_to_first_packet_ms: Option<u64>,
    pub legacy_time_to_first_packet_ms: Option<u64>,
}

impl RendezvousMetrics {
    /// Create a zeroed metrics snapshot.
    pub fn new() -> Self {
        Self {
            slots_attempted: 0,
            slots_succeeded: None,
            total_duration_ms: 0,
            probes_sent: 0,
            probes_received: 0,
            escalations_triggered: 0,
            nat_behavior_notes: Vec::new(),
            fallback_used: false,
            fallback_method: None,
            time_to_first_packet_ms: None,
            legacy_time_to_first_packet_ms: None,
        }
    }
}
