/// Deterministic search strategy used for rendezvous scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchStrategy {
    /// Basic deterministic schedule traversal.
    BasicDeterministic,
    /// Extended deterministic strategy (placeholder).
    Extended,
}

/// Policy describing how the search escalates over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EscalationPolicy {
    /// No escalation (fixed parameters).
    None,
    /// Simple escalation (placeholder).
    Simple,
    /// Aggressive escalation (placeholder).
    Aggressive,
}
