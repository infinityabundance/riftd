//! rndzv: Predictive Rendezvous networking layer for riftd.
//! Provides Semantic Rendezvous Tokens (SRTs), deterministic rendezvous
//! scheduling, and higher-level session/channel abstractions.

pub mod api;
pub mod config;
pub mod engine;
pub mod schedule;
pub mod srt;
pub mod time;

pub use api::{
    ChannelId, ChannelKind, DhtHints, PathBinding, PeerId, RendezvousSpaceId, RndzvChannel,
    RndzvClient, RndzvConnectTarget, RndzvConnector, RndzvError, RndzvListener, RndzvOutcome,
    RndzvSession, SessionId, Srt,
};
pub use config::HybridMode;
pub use engine::{
    build_probe_payload, parse_probe_payload, rendezvous_id_from_seed, validate_probe_for_token,
    Clock, HybridPath, NatBehaviorHint, ParsedProbe, ProbeError, ProbePayload, RendezvousError,
    RendezvousMetrics, RendezvousOutcome, RendezvousRunner, RendezvousState, UdpIo,
};
pub use schedule::{compute_slot_params, Role, SlotParams};
pub use srt::{EscalationPolicy, IdentityConstraints, SearchStrategy, SemanticRendezvousToken, SrtError};
pub use time::TimeModel;
