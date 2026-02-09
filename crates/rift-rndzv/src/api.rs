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
#[derive(Debug)]
pub struct PathBinding {
    /// Local UDP socket bound for this session.
    pub local_socket: std::net::UdpSocket,
    /// Remote socket address selected for the session.
    pub remote_addr: std::net::SocketAddr,
    // TODO: add crypto context later.
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
#[derive(Debug)]
pub struct RndzvSession {
    pub id: SessionId,
    pub local: PeerId,
    pub remote: PeerId,
    pub space: RendezvousSpaceId,
    // Underlying path/transport binding to be filled in later.
    pub path: PathBinding,
}

impl RndzvSession {
    /// Create a new session with a path binding.
    pub fn new(
        id: SessionId,
        local: PeerId,
        remote: PeerId,
        space: RendezvousSpaceId,
        path: PathBinding,
    ) -> Self {
        Self {
            id,
            local,
            remote,
            space,
            path,
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
    Timeout,
    Io(std::io::Error),
}

impl fmt::Display for RndzvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RndzvError::NotImplemented(op) => write!(f, "not implemented: {op}"),
            RndzvError::InvalidState(msg) => write!(f, "invalid state: {msg}"),
            RndzvError::Transport(msg) => write!(f, "transport error: {msg}"),
            RndzvError::Timeout => write!(f, "rendezvous timed out"),
            RndzvError::Io(err) => write!(f, "io error: {err}"),
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
    remote_addrs: Vec<std::net::SocketAddr>,
    local_bind: Option<std::net::SocketAddr>,
    timeout: std::time::Duration,
}

impl RndzvConnector {
    /// Create a new connector instance.
    pub fn new() -> Self {
        Self {
            remote_addrs: Vec::new(),
            local_bind: None,
            timeout: std::time::Duration::from_secs(5),
        }
    }

    /// Provide candidate remote addresses to probe.
    pub fn with_remote_addrs(mut self, addrs: Vec<std::net::SocketAddr>) -> Self {
        self.remote_addrs = addrs;
        self
    }

    /// Bind the local UDP socket to a specific address.
    pub fn with_local_bind(mut self, addr: std::net::SocketAddr) -> Self {
        self.local_bind = Some(addr);
        self
    }

    /// Override the rendezvous timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Connect using a rendezvous target (stub).
    pub async fn connect(
        &self,
        target: RndzvConnectTarget,
    ) -> Result<RndzvSession, RndzvError> {
        use crate::engine::{
            build_probe_payload, parse_probe_payload, rendezvous_id_from_seed,
            validate_probe_for_token, ProbePayload,
        };
        use crate::schedule::{compute_slot_params, Role};

        let bind_addr = self
            .local_bind
            .unwrap_or_else(|| std::net::SocketAddr::from(([0, 0, 0, 0], 0)));
        let socket = std::net::UdpSocket::bind(bind_addr).map_err(RndzvError::Io)?;
        socket
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .map_err(RndzvError::Io)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| RndzvError::InvalidState("system clock before unix epoch"))?;
        let start_ms = now.as_millis() as u64;
        let deadline_ms = start_ms.saturating_add(self.timeout.as_millis() as u64);

        let mut sender_fingerprint = [0u8; 16];
        sender_fingerprint.copy_from_slice(&target.local_peer.0[..16]);
        let rendezvous_id = rendezvous_id_from_seed(&target.srt.seed);

        let mut last_slot: Option<u64> = None;
        let mut buf = [0u8; 1500];

        loop {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| RndzvError::InvalidState("system clock before unix epoch"))?
                .as_millis() as u64;

            if now_ms >= deadline_ms {
                return Err(RndzvError::Timeout);
            }

            if let Some(slot) = compute_slot_params(
                &target.srt.seed,
                &target.srt.time_model,
                Role::Caller,
                now_ms,
            ) {
                if last_slot != Some(slot.slot_index) {
                    last_slot = Some(slot.slot_index);
                    let payload = build_probe_payload(ProbePayload {
                        rendezvous_id,
                        slot_index: slot.slot_index,
                        sender_fingerprint,
                    });
                    for addr in &self.remote_addrs {
                        let _ = socket.send_to(&payload, addr);
                    }
                }
            }

            match socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    if let Ok(parsed) = parse_probe_payload(&buf[..len]) {
                        if validate_probe_for_token(&target.srt, &parsed) {
                            let mut remote_bytes = [0u8; 32];
                            remote_bytes[..16].copy_from_slice(&parsed.sender_fingerprint);
                            let remote_peer = PeerId(remote_bytes);
                            let session_id = SessionId(rendezvous_id.to_le_bytes());
                            let path = PathBinding {
                                local_socket: socket,
                                remote_addr: addr,
                            };
                            return Ok(RndzvSession::new(
                                session_id,
                                target.local_peer,
                                remote_peer,
                                target.srt.space,
                                path,
                            ));
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => return Err(RndzvError::Io(err)),
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
/// let srt = Srt::from_uri("riftd-srt://v1?space=0000000000000000000000000000000000000000000000000000000000000000&seed=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&t0=0&tw=1&slot=1&ss=basic&esc=none")?;
/// let target = RndzvConnectTarget::from_srt(srt, local);
/// let connector = RndzvConnector::new();
/// let session = connector.connect(target).await?;
/// let _ch = session.open_channel(rift_rndzv::ChannelKind::ReliableOrdered).await?;
/// # Ok(())
/// # }
/// ```

#[cfg(test)]
mod tests {
    use super::*;
    use crate::srt::{EscalationPolicy, IdentityConstraints, SearchStrategy};
    use crate::time::TimeModel;
    use futures::executor::block_on;
    use std::net::SocketAddr;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn connect_loopback_succeeds() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let srt = Srt::new(
            RendezvousSpaceId([0u8; 32]),
            [7u8; 32],
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0: now + 1,
                window_secs: 3,
                slot_ms: 50,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        );

        let addr_a: SocketAddr = "127.0.0.1:40001".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:40002".parse().unwrap();

        let srt_a = srt.clone();
        let srt_b = srt.clone();

        let handle_a = thread::spawn(move || {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_a)
                .with_remote_addrs(vec![addr_b])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_a, PeerId([1u8; 32]));
            block_on(connector.connect(target))
        });

        let handle_b = thread::spawn(move || {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_b)
                .with_remote_addrs(vec![addr_a])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_b, PeerId([2u8; 32]));
            block_on(connector.connect(target))
        });

        let res_a = handle_a.join().expect("thread a");
        let res_b = handle_b.join().expect("thread b");

        assert!(res_a.is_ok(), "connect A failed: {res_a:?}");
        assert!(res_b.is_ok(), "connect B failed: {res_b:?}");
    }
}
