//! Rendezvous runner skeleton and probe emission.

use crate::{compute_slot_params, rendezvous_id_from_seed, Role, SemanticRendezvousToken, SlotParams};
use std::fmt;
use std::net::SocketAddr;

/// Abstraction over time for testability.
pub trait Clock: Send + Sync {
    /// Current Unix time in milliseconds.
    fn now_unix_ms(&self) -> u64;
}

/// Abstraction over UDP IO for testability.
pub trait UdpIo: Send + Sync {
    /// IO error type.
    type Error;

    /// Send a probe payload using a local port offset and target address.
    fn send_probe(
        &self,
        local_port: u16,
        remote_addr: SocketAddr,
        payload: &[u8],
    ) -> Result<(), Self::Error>;
}

/// Payload fields used during rendezvous probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbePayload {
    /// Deterministic rendezvous identifier.
    pub rendezvous_id: u64,
    /// Slot index for the probe.
    pub slot_index: u64,
    /// Truncated sender fingerprint (placeholder).
    pub sender_fingerprint: [u8; 16],
}

/// Build a compact probe payload.
///
/// Format: `magic[4] || rendezvous_id(u64 LE) || slot_index(u64 LE) || sender_fingerprint[16]`.
pub fn build_probe_payload(payload: ProbePayload) -> Vec<u8> {
    const MAGIC: [u8; 4] = *b"PRV1";
    let mut out = Vec::with_capacity(4 + 8 + 8 + 16);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&payload.rendezvous_id.to_le_bytes());
    out.extend_from_slice(&payload.slot_index.to_le_bytes());
    out.extend_from_slice(&payload.sender_fingerprint);
    out
}

/// Errors emitted by the rendezvous runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendezvousError<E> {
    /// Failed to send a probe via UDP.
    Udp(E),
}

impl<E: fmt::Display> fmt::Display for RendezvousError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendezvousError::Udp(err) => write!(f, "udp send failed: {err}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for RendezvousError<E> {}

/// Stateless runner for emitting rendezvous probes.
pub struct RendezvousRunner<C: Clock, U: UdpIo> {
    /// Semantic Rendezvous Token.
    pub token: SemanticRendezvousToken,
    /// Schedule derivation role.
    pub role: Role,
    /// Time source.
    pub clock: C,
    /// UDP IO abstraction.
    pub udp: U,
    /// Sender fingerprint (truncated placeholder).
    pub sender_fingerprint: [u8; 16],
}

impl<C: Clock, U: UdpIo> RendezvousRunner<C, U> {
    /// Emit probes for the current slot (if any).
    pub fn run_slot_once(
        &self,
        potential_remote_addrs: &[SocketAddr],
    ) -> Result<Option<SlotParams>, RendezvousError<U::Error>> {
        let now = self.clock.now_unix_ms();
        let slot_params = match compute_slot_params(
            &self.token.seed,
            &self.token.time_model,
            self.role,
            now,
        ) {
            Some(params) => params,
            None => return Ok(None),
        };

        let rendezvous_id = rendezvous_id_from_seed(&self.token.seed);
        let payload = build_probe_payload(ProbePayload {
            rendezvous_id,
            slot_index: slot_params.slot_index,
            sender_fingerprint: self.sender_fingerprint,
        });

        for addr in potential_remote_addrs {
            self.udp
                .send_probe(slot_params.local_port_offset, *addr, &payload)
                .map_err(RendezvousError::Udp)?;
        }

        Ok(Some(slot_params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdentityConstraints, SearchStrategy, TimeModel, EscalationPolicy};
    use std::sync::{Arc, Mutex};

    struct MockClock {
        now: u64,
    }

    impl Clock for MockClock {
        fn now_unix_ms(&self) -> u64 {
            self.now
        }
    }

    #[derive(Default)]
    struct MockUdpIo {
        sent: Arc<Mutex<Vec<(u16, SocketAddr, Vec<u8>)>>>,
    }

    impl UdpIo for MockUdpIo {
        type Error = ();

        fn send_probe(
            &self,
            local_port: u16,
            remote_addr: SocketAddr,
            payload: &[u8],
        ) -> Result<(), Self::Error> {
            self.sent
                .lock()
                .expect("lock")
                .push((local_port, remote_addr, payload.to_vec()));
            Ok(())
        }
    }

    fn base_token() -> SemanticRendezvousToken {
        SemanticRendezvousToken::new(
            crate::api::RendezvousSpaceId([0u8; 32]),
            [7u8; 32],
            IdentityConstraints {
                allowed_fingerprints: vec![],
            },
            TimeModel {
                t0: 1_700_000_000,
                window_secs: 10,
                slot_ms: 250,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        )
    }

    #[test]
    fn run_slot_sends_probes() {
        let token = base_token();
        let now = token.time_model.t0 * 1_000 + 500;
        let clock = MockClock { now };
        let udp = MockUdpIo::default();
        let sent = udp.sent.clone();

        let runner = RendezvousRunner {
            token,
            role: Role::Caller,
            clock,
            udp,
            sender_fingerprint: [9u8; 16],
        };

        let addr1: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:9001".parse().unwrap();

        let slot = runner
            .run_slot_once(&[addr1, addr2])
            .expect("run slot")
            .expect("slot params");

        let sent = sent.lock().expect("lock");
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, slot.local_port_offset);
        assert_eq!(sent[1].0, slot.local_port_offset);
        assert_eq!(sent[0].1, addr1);
        assert_eq!(sent[1].1, addr2);
        assert!(!sent[0].2.is_empty());
    }

    #[test]
    fn run_slot_out_of_window_sends_nothing() {
        let mut token = base_token();
        token.time_model.window_secs = 1;
        let now = token.time_model.t0 * 1_000 + 2_000;

        let clock = MockClock { now };
        let udp = MockUdpIo::default();
        let sent = udp.sent.clone();

        let runner = RendezvousRunner {
            token,
            role: Role::Caller,
            clock,
            udp,
            sender_fingerprint: [1u8; 16],
        };

        let addr: SocketAddr = "127.0.0.1:9002".parse().unwrap();
        let slot = runner
            .run_slot_once(&[addr])
            .expect("run slot");

        assert!(slot.is_none());
        assert!(sent.lock().expect("lock").is_empty());
    }
}
