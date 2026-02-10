//! Configuration types for hybrid rendezvous modes.

/// Coordination mode for combining rndzv with optional infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum HybridMode {
    /// Pure Predictive Rendezvous (no infrastructure hints or fallback).
    #[default]
    PureRndzv,
    /// Attempt rndzv first, then fall back to relay if needed.
    RndzvThenRelay,
    /// Attempt rndzv and relay in parallel, use the first success.
    ParallelRndzvAndRelay,
    /// Use DHT hints to seed candidate addresses, but keep rndzv canonical.
    RndzvWithDhtHints,
}

