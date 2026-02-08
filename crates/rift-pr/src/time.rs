/// Time model governing rendezvous slotting and search windows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimeModel {
    /// Anchor time in Unix seconds.
    pub t0: u64,
    /// Window size in seconds (Tw).
    pub window_secs: u64,
    /// Slot duration in milliseconds (Δ).
    pub slot_ms: u64,
}
