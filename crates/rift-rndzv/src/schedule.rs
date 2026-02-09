//! Deterministic schedule derivation for Predictive Rendezvous.

use crate::TimeModel;

/// Role used to derive asymmetric schedules between peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiating side of the rendezvous.
    Caller,
    /// Receiving side of the rendezvous.
    Callee,
    /// Symmetric derivation (both sides use the same role tag).
    Symmetric,
}

/// Per-slot schedule parameters derived from seed + role + slot index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotParams {
    /// Slot index `n` within the rendezvous window.
    pub slot_index: u64,
    /// Local port offset (not an absolute port).
    pub local_port_offset: u16,
    /// Remote port offset (not an absolute port).
    pub remote_port_offset: u16,
    /// Burst pattern for this slot.
    pub burst_pattern: [u8; 4],
}

/// Compute deterministic slot parameters for the current time.
///
/// This function is the core of Predictive Rendezvous. It is used by both peers
/// to independently derive the same slot schedule without any server or
/// negotiation.
pub fn compute_slot_params(
    seed: &[u8; 32],
    time_model: &TimeModel,
    role: Role,
    now_unix_ms: u64,
) -> Option<SlotParams> {
    if time_model.slot_ms == 0 {
        return None;
    }

    let t0_ms = time_model.t0.saturating_mul(1_000);
    if now_unix_ms < t0_ms {
        return None;
    }

    let window_ms = time_model.window_secs.saturating_mul(1_000);
    let end_ms = t0_ms.saturating_add(window_ms);
    if now_unix_ms >= end_ms {
        return None;
    }

    let slot_index = (now_unix_ms - t0_ms) / time_model.slot_ms;

    let mut hasher = blake3::Hasher::new();
    hasher.update(seed);
    hasher.update(match role {
        Role::Caller => b"caller",
        Role::Callee => b"callee",
        Role::Symmetric => b"symmetric",
    });
    hasher.update(&slot_index.to_le_bytes());
    let digest = hasher.finalize();
    let bytes = digest.as_bytes();

    let local_port_offset = u16::from_le_bytes([bytes[0], bytes[1]]);
    let remote_port_offset = u16::from_le_bytes([bytes[2], bytes[3]]);
    let burst_pattern = [bytes[4], bytes[5], bytes[6], bytes[7]];

    Some(SlotParams {
        slot_index,
        local_port_offset,
        remote_port_offset,
        burst_pattern,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_time_model() -> TimeModel {
        TimeModel {
            t0: 1_700_000_000,
            window_secs: 60,
            slot_ms: 250,
        }
    }

    #[test]
    fn determinism_same_inputs() {
        let seed = [8u8; 32];
        let time_model = base_time_model();
        let now = time_model.t0 * 1_000 + 1_000;

        let first = compute_slot_params(&seed, &time_model, Role::Caller, now)
            .expect("slot params");
        let second = compute_slot_params(&seed, &time_model, Role::Caller, now)
            .expect("slot params");

        assert_eq!(first, second);
    }

    #[test]
    fn different_roles_yield_different_params() {
        let seed = [9u8; 32];
        let time_model = base_time_model();
        let now = time_model.t0 * 1_000 + 2_000;

        let caller = compute_slot_params(&seed, &time_model, Role::Caller, now)
            .expect("caller slot");
        let callee = compute_slot_params(&seed, &time_model, Role::Callee, now)
            .expect("callee slot");

        assert_ne!(caller, callee);
    }

    #[test]
    fn different_seeds_yield_different_params() {
        let time_model = base_time_model();
        let now = time_model.t0 * 1_000 + 3_000;

        let a = compute_slot_params(&[1u8; 32], &time_model, Role::Symmetric, now)
            .expect("slot params");
        let b = compute_slot_params(&[2u8; 32], &time_model, Role::Symmetric, now)
            .expect("slot params");

        assert_ne!(a, b);
    }

    #[test]
    fn out_of_window_returns_none() {
        let seed = [3u8; 32];
        let time_model = base_time_model();

        let before = time_model.t0 * 1_000 - 1;
        assert!(compute_slot_params(&seed, &time_model, Role::Caller, before).is_none());

        let end = time_model.t0 * 1_000 + time_model.window_secs * 1_000;
        assert!(compute_slot_params(&seed, &time_model, Role::Caller, end).is_none());
    }
}
