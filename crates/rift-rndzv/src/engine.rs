//! Low-level Predictive Rendezvous engine components.

pub mod metrics;
pub mod probe;
pub mod runner;
pub mod sim;

pub use metrics::{NatBehaviorHint, RendezvousMetrics};
pub use probe::{
    parse_probe_payload, rendezvous_id_from_seed, validate_probe_for_token, ParsedProbe, ProbeError,
    RendezvousOutcome, RendezvousState,
};
pub use runner::{build_probe_payload, Clock, ProbePayload, RendezvousError, RendezvousRunner, UdpIo};
