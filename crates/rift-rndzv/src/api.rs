//! Higher-level networking/session API (stub).

use std::fmt;

/// Stable identifier for a peer in the rndzv layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PeerId(pub [u8; 32]);

/// Logical coordination namespace for a rendezvous session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RendezvousSpaceId(pub [u8; 32]);

/// Public alias for Semantic Rendezvous Tokens.
pub use crate::srt::SemanticRendezvousToken as Srt;

/// Stable identifier for a rendezvous session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(pub [u8; 16]);

/// Underlying path/transport binding (placeholder).
#[derive(Clone, Debug)]
pub struct PathBinding {
    _private: (),
}

impl PathBinding {
    /// Create a placeholder binding.
    pub fn new() -> Self {
        Self { _private: () }
    }
}

/// Logical channel semantics for a session.
#[derive(Clone, Copy, Debug)]
pub enum ChannelKind {
    /// Reliable, ordered delivery.
    ReliableOrdered,
    /// Unreliable datagrams.
    UnreliableDatagram,
}

/// Identifier for a channel within a session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ChannelId(pub u32);

/// Rndzv session handle.
#[derive(Clone, Debug)]
pub struct RndzvSession {
    pub id: SessionId,
    pub local: PeerId,
    pub remote: PeerId,
    pub space: RendezvousSpaceId,
    // Underlying path/transport binding to be filled in later.
    path: PathBinding,
}

impl RndzvSession {
    /// Create a new session with a placeholder path binding.
    pub fn new(id: SessionId, local: PeerId, remote: PeerId, space: RendezvousSpaceId) -> Self {
        Self {
            id,
            local,
            remote,
            space,
            path: PathBinding::new(),
        }
    }

    /// Return the session identifier.
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Return the local peer identifier.
    pub fn local_peer(&self) -> PeerId {
        self.local
    }

    /// Return the remote peer identifier.
    pub fn remote_peer(&self) -> PeerId {
        self.remote
    }

    /// Return the rendezvous space identifier.
    pub fn space(&self) -> RendezvousSpaceId {
        self.space
    }

    /// Open a logical channel (stub).
    pub async fn open_channel(&self, kind: ChannelKind) -> Result<RndzvChannel, RndzvError> {
        let _ = kind;
        Err(RndzvError::NotImplemented("open_channel"))
    }
}

/// Rndzv logical channel handle.
#[derive(Clone, Debug)]
pub struct RndzvChannel {
    pub id: ChannelId,
    pub kind: ChannelKind,
    session_id: SessionId,
}

impl RndzvChannel {
    /// Send data on this channel (stub).
    pub async fn send(&self, _data: &[u8]) -> Result<(), RndzvError> {
        Err(RndzvError::NotImplemented("send"))
    }

    /// Receive data from this channel (stub).
    pub async fn recv(&self) -> Result<Option<Vec<u8>>, RndzvError> {
        Err(RndzvError::NotImplemented("recv"))
    }
}

/// Errors returned by rndzv session and channel APIs.
#[derive(Debug)]
pub enum RndzvError {
    NotImplemented(&'static str),
    InvalidState(&'static str),
    Transport(&'static str),
}

impl fmt::Display for RndzvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RndzvError::NotImplemented(op) => write!(f, "not implemented: {op}"),
            RndzvError::InvalidState(msg) => write!(f, "invalid state: {msg}"),
            RndzvError::Transport(msg) => write!(f, "transport error: {msg}"),
        }
    }
}

impl std::error::Error for RndzvError {}

/// Target parameters for establishing a rendezvous session.
pub struct RndzvConnectTarget {
    pub srt: Srt,
    pub local_peer: PeerId,
}

impl RndzvConnectTarget {
    /// Build a connect target from an SRT and local peer id.
    pub fn from_srt(srt: Srt, local_peer: PeerId) -> Self {
        Self { srt, local_peer }
    }
}

/// High-level connector for establishing rndzv sessions.
pub struct RndzvConnector {
    // later: config, handles to transport, etc.
    _private: (),
}

impl RndzvConnector {
    /// Create a new connector instance.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Connect using a rendezvous target (stub).
    pub async fn connect(
        &self,
        _target: RndzvConnectTarget,
    ) -> Result<RndzvSession, RndzvError> {
        Err(RndzvError::NotImplemented("connect"))
    }
}

/// Listener for inbound rndzv sessions.
pub struct RndzvListener {
    space: RendezvousSpaceId,
    local_peer: PeerId,
    // later: references to PR engine, UDP sockets, etc.
}

impl RndzvListener {
    /// Create a new listener for a rendezvous space.
    pub fn new(space: RendezvousSpaceId, local_peer: PeerId) -> Self {
        Self { space, local_peer }
    }

    /// Accept an incoming rendezvous session (stub).
    pub async fn accept(&self) -> Result<RndzvSession, RndzvError> {
        Err(RndzvError::NotImplemented("accept"))
    }
}

/// Placeholder for future session/channel APIs.
#[derive(Debug, Default)]
pub struct RndzvClient;

impl RndzvClient {
    /// Create a new client instance.
    pub fn new() -> Self {
        Self
    }
}

/// Example showing the intended connect flow.
///
/// ```ignore
/// use rift_rndzv::{
///     PeerId, RendezvousSpaceId, RndzvConnector, RndzvConnectTarget, Srt,
/// };
///
/// # async fn example() -> Result<(), rift_rndzv::RndzvError> {
/// let local = PeerId([0u8; 32]);
/// let srt = Srt::from_uri("riftd-srt://v1?space=00&seed=00&t0=0&tw=1&slot=1&ss=basic&esc=none")?;
/// let target = RndzvConnectTarget::from_srt(srt, local);
/// let connector = RndzvConnector::new();
/// let session = connector.connect(target).await?;
/// let _ch = session.open_channel(rift_rndzv::ChannelKind::ReliableOrdered).await?;
/// # Ok(())
/// # }
/// ```
