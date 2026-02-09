//! Higher-level networking/session API (stub).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex, watch};
use tokio::task::JoinHandle;

/// Stable identifier for a peer in the rndzv layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PeerId(pub [u8; 32]);

/// Logical coordination namespace for a rendezvous session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RendezvousSpaceId(pub [u8; 32]);

/// Public alias for Semantic Rendezvous Tokens.
pub use crate::srt::SemanticRendezvousToken as Srt;

/// Stable identifier for a rendezvous session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(pub [u8; 16]);

/// Underlying path/transport binding (placeholder).
#[derive(Debug)]
pub struct PathBinding {
    /// Local UDP socket bound for this session.
    pub socket: Arc<UdpSocket>,
    /// Remote socket address selected for the session.
    pub remote_addr: std::net::SocketAddr,
    /// Demux state for channel receivers.
    demux: Arc<Mutex<DemuxState>>,
    // TODO: add crypto context later.
}

#[derive(Debug)]
struct DemuxState {
    channels: HashMap<ChannelId, mpsc::Sender<Vec<u8>>>,
    next_channel: u32,
}

impl DemuxState {
    fn new() -> Self {
        Self {
            channels: HashMap::new(),
            next_channel: 1,
        }
    }

    fn next_channel_id(&mut self) -> ChannelId {
        let id = ChannelId(self.next_channel);
        self.next_channel = self.next_channel.wrapping_add(1);
        id
    }

    fn register(&mut self, id: ChannelId, tx: mpsc::Sender<Vec<u8>>) {
        self.channels.insert(id, tx);
    }

    fn unregister(&mut self, id: ChannelId) {
        self.channels.remove(&id);
    }
}

fn start_demux_task(
    socket: Arc<UdpSocket>,
    state: Arc<Mutex<DemuxState>>,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                res = socket.recv_from(&mut buf) => {
                    let (len, _addr) = match res {
                        Ok(res) => res,
                        Err(_) => break,
                    };
                    if let Ok((channel_id, payload)) = decode_frame(&buf[..len]) {
                        let tx = {
                            let guard = state.lock().await;
                            guard.channels.get(&channel_id).cloned()
                        };
                        if let Some(tx) = tx {
                            let _ = tx.send(payload).await;
                        }
                    }
                }
            }
        }
    })
}

fn encode_frame(channel_id: ChannelId, payload: &[u8]) -> Result<Vec<u8>, RndzvError> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&channel_id.0.to_le_bytes());
    let len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| RndzvError::InvalidState("payload too large"))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn decode_frame(input: &[u8]) -> Result<(ChannelId, Vec<u8>), RndzvError> {
    if input.len() < 8 {
        return Err(RndzvError::InvalidState("frame too short"));
    }
    let mut id_bytes = [0u8; 4];
    id_bytes.copy_from_slice(&input[..4]);
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&input[4..8]);
    let channel_id = ChannelId(u32::from_le_bytes(id_bytes));
    let len = u32::from_le_bytes(len_bytes) as usize;
    if input.len() < 8 + len {
        return Err(RndzvError::InvalidState("frame length mismatch"));
    }
    Ok((channel_id, input[8..8 + len].to_vec()))
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
    pub path: Arc<PathBinding>,
    shutdown_tx: watch::Sender<bool>,
    demux_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl RndzvSession {
    /// Create a new session with a path binding.
    pub fn new(
        id: SessionId,
        local: PeerId,
        remote: PeerId,
        space: RendezvousSpaceId,
        path: PathBinding,
        shutdown_tx: watch::Sender<bool>,
        demux_task: JoinHandle<()>,
    ) -> Self {
        Self {
            id,
            local,
            remote,
            space,
            path: Arc::new(path),
            shutdown_tx,
            demux_task: Arc::new(Mutex::new(Some(demux_task))),
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
        let kind = match kind {
            ChannelKind::ReliableOrdered => ChannelKind::UnreliableDatagram,
            other => other,
        };
        let channel_id = {
            let mut demux = self.path.demux.lock().await;
            demux.next_channel_id()
        };
        let (tx, rx) = mpsc::channel(64);
        {
            let mut demux = self.path.demux.lock().await;
            demux.register(channel_id, tx);
        }
        Ok(RndzvChannel {
            id: channel_id,
            kind,
            path: self.path.clone(),
            rx: Arc::new(Mutex::new(rx)),
        })
    }

    /// Shut down the session and stop the demux task.
    pub async fn shutdown(&self) -> Result<(), RndzvError> {
        let _ = self.shutdown_tx.send(true);
        let mut task = self.demux_task.lock().await;
        if let Some(handle) = task.take() {
            let _ = handle.await;
        }
        Ok(())
    }
}

impl Drop for RndzvSession {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Rndzv logical channel handle.
#[derive(Clone, Debug)]
pub struct RndzvChannel {
    pub id: ChannelId,
    pub kind: ChannelKind,
    path: Arc<PathBinding>,
    rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

impl RndzvChannel {
    /// Send data on this channel (stub).
    pub async fn send(&self, data: &[u8]) -> Result<(), RndzvError> {
        let frame = encode_frame(self.id, data)?;
        self.path
            .socket
            .send_to(&frame, self.path.remote_addr)
            .await
            .map_err(RndzvError::Io)?;
        Ok(())
    }

    /// Receive data from this channel (stub).
    pub async fn recv(&self) -> Result<Option<Vec<u8>>, RndzvError> {
        let mut rx = self.rx.lock().await;
        Ok(rx.recv().await)
    }

    /// Close this channel and unregister it.
    pub async fn close(&self) -> Result<(), RndzvError> {
        let mut demux = self.path.demux.lock().await;
        demux.unregister(self.id);
        Ok(())
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

fn derive_base_port(seed: &[u8; 32]) -> u16 {
    let raw = u16::from_le_bytes([seed[0], seed[1]]);
    let range: u16 = 20_000;
    40_000u16.saturating_add(raw % range)
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

        let base_port = derive_base_port(&target.srt.seed);
        let use_broadcast = self.remote_addrs.is_empty();
        let bind_addr = self
            .local_bind
            .unwrap_or_else(|| std::net::SocketAddr::from(([0, 0, 0, 0], 0)));
        let socket = UdpSocket::bind(bind_addr).await.map_err(RndzvError::Io)?;
        if use_broadcast {
            socket.set_broadcast(true).map_err(RndzvError::Io)?;
        }

        let start_instant = tokio::time::Instant::now();
        let deadline = start_instant + self.timeout;

        let mut sender_fingerprint = [0u8; 16];
        sender_fingerprint.copy_from_slice(&target.local_peer.0[..16]);
        let rendezvous_id = rendezvous_id_from_seed(&target.srt.seed);

        let mut last_slot: Option<u64> = None;
        let mut buf = [0u8; 1500];

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(RndzvError::Timeout);
            }

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| RndzvError::InvalidState("system clock before unix epoch"))?
                .as_millis() as u64;

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
                    if use_broadcast {
                        let port = base_port.wrapping_add(slot.remote_port_offset);
                        let addr = std::net::SocketAddr::from(([255, 255, 255, 255], port));
                        let _ = socket.send_to(&payload, addr).await;
                    } else {
                        for addr in &self.remote_addrs {
                            let _ = socket.send_to(&payload, addr).await;
                        }
                    }
                }
            }

            match tokio::time::timeout(std::time::Duration::from_millis(50), socket.recv_from(&mut buf)).await {
                Ok(Ok((len, addr))) => {
                    if let Ok(parsed) = parse_probe_payload(&buf[..len]) {
                        if validate_probe_for_token(&target.srt, &parsed) {
                            let mut remote_bytes = [0u8; 32];
                            remote_bytes[..16].copy_from_slice(&parsed.sender_fingerprint);
                            let remote_peer = PeerId(remote_bytes);
                            let mut session_id = [0u8; 16];
                            session_id[..8].copy_from_slice(&rendezvous_id.to_le_bytes());
                            let session_id = SessionId(session_id);
                            let socket = Arc::new(socket);
                            let demux = Arc::new(Mutex::new(DemuxState::new()));
                            let (shutdown_tx, shutdown_rx) = watch::channel(false);
                            let demux_task = start_demux_task(socket.clone(), demux.clone(), shutdown_rx);
                            let path = PathBinding {
                                socket,
                                remote_addr: addr,
                                demux,
                            };
                            return Ok(RndzvSession::new(
                                session_id,
                                target.local_peer,
                                remote_peer,
                                target.srt.space,
                                path,
                                shutdown_tx,
                                demux_task,
                            ));
                        }
                    }
                }
                Ok(Err(err)) => return Err(RndzvError::Io(err)),
                Err(_) => {}
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// Listener for inbound rndzv sessions.
pub struct RndzvListener {
    space: RendezvousSpaceId,
    local_peer: PeerId,
    srt: Option<Srt>,
    local_bind: Option<std::net::SocketAddr>,
    timeout: std::time::Duration,
    // later: references to PR engine, UDP sockets, etc.
}

impl RndzvListener {
    /// Create a new listener for a rendezvous space.
    pub fn new(space: RendezvousSpaceId, local_peer: PeerId) -> Self {
        Self {
            space,
            local_peer,
            srt: None,
            local_bind: None,
            timeout: std::time::Duration::from_secs(5),
        }
    }

    /// Provide the SRT used to validate incoming probes.
    pub fn with_srt(mut self, srt: Srt) -> Self {
        self.srt = Some(srt);
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

    /// Accept an incoming rendezvous session (stub).
    pub async fn accept(&self) -> Result<RndzvSession, RndzvError> {
        use crate::engine::{
            build_probe_payload, parse_probe_payload, rendezvous_id_from_seed,
            validate_probe_for_token, ProbePayload,
        };

        let srt = self
            .srt
            .as_ref()
            .ok_or(RndzvError::InvalidState("listener missing SRT"))?;
        if srt.space != self.space {
            return Err(RndzvError::InvalidState("listener space mismatch"));
        }

        let base_port = derive_base_port(&srt.seed);
        let bind_addr = self
            .local_bind
            .unwrap_or_else(|| std::net::SocketAddr::from(([0, 0, 0, 0], base_port)));
        let socket = UdpSocket::bind(bind_addr).await.map_err(RndzvError::Io)?;

        let start_instant = tokio::time::Instant::now();
        let deadline = start_instant + self.timeout;

        let rendezvous_id = rendezvous_id_from_seed(&srt.seed);
        let mut sender_fingerprint = [0u8; 16];
        sender_fingerprint.copy_from_slice(&self.local_peer.0[..16]);

        let mut buf = [0u8; 1500];
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(RndzvError::Timeout);
            }

            match tokio::time::timeout(std::time::Duration::from_millis(50), socket.recv_from(&mut buf)).await {
                Ok(Ok((len, addr))) => {
                    if let Ok(parsed) = parse_probe_payload(&buf[..len]) {
                        if validate_probe_for_token(srt, &parsed) {
                            let response = build_probe_payload(ProbePayload {
                                rendezvous_id,
                                slot_index: parsed.slot_index,
                                sender_fingerprint,
                            });
                            let _ = socket.send_to(&response, addr).await;

                            let mut remote_bytes = [0u8; 32];
                            remote_bytes[..16].copy_from_slice(&parsed.sender_fingerprint);
                            let remote_peer = PeerId(remote_bytes);
                            let mut session_id = [0u8; 16];
                            session_id[..8].copy_from_slice(&rendezvous_id.to_le_bytes());
                            let session_id = SessionId(session_id);
                            let socket = Arc::new(socket);
                            let demux = Arc::new(Mutex::new(DemuxState::new()));
                            let (shutdown_tx, shutdown_rx) = watch::channel(false);
                            let demux_task = start_demux_task(socket.clone(), demux.clone(), shutdown_rx);
                            let path = PathBinding {
                                socket,
                                remote_addr: addr,
                                demux,
                            };
                            return Ok(RndzvSession::new(
                                session_id,
                                self.local_peer,
                                remote_peer,
                                self.space,
                                path,
                                shutdown_tx,
                                demux_task,
                            ));
                        }
                    }
                }
                Ok(Err(err)) => return Err(RndzvError::Io(err)),
                Err(_) => {}
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
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
    use std::net::SocketAddr;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn connect_loopback_succeeds() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let srt_a = Srt::new(
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
        let srt_b = srt_a.clone();

        let addr_a: SocketAddr = "127.0.0.1:40001".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:40002".parse().unwrap();

        let handle_a = tokio::spawn(async move {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_a)
                .with_remote_addrs(vec![addr_b])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_a, PeerId([1u8; 32]));
            connector.connect(target).await
        });

        let handle_b = tokio::spawn(async move {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_b)
                .with_remote_addrs(vec![addr_a])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_b, PeerId([2u8; 32]));
            connector.connect(target).await
        });

        let (res_a, res_b) = tokio::join!(handle_a, handle_b);
        let res_a = res_a.expect("task a");
        let res_b = res_b.expect("task b");

        assert!(res_a.is_ok(), "connect A failed: {res_a:?}");
        assert!(res_b.is_ok(), "connect B failed: {res_b:?}");
    }

    #[tokio::test]
    async fn listener_accepts_connector() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let srt = Srt::new(
            RendezvousSpaceId([9u8; 32]),
            [5u8; 32],
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

        let addr_listener: SocketAddr = "127.0.0.1:40101".parse().unwrap();
        let addr_connector: SocketAddr = "127.0.0.1:40102".parse().unwrap();

        let srt_listener = srt.clone();
        let srt_connector = srt.clone();

        let handle_listener = tokio::spawn(async move {
            let listener = RndzvListener::new(RendezvousSpaceId([9u8; 32]), PeerId([3u8; 32]))
                .with_srt(srt_listener)
                .with_local_bind(addr_listener)
                .with_timeout(Duration::from_secs(3));
            listener.accept().await
        });

        let handle_connector = tokio::spawn(async move {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_connector)
                .with_remote_addrs(vec![addr_listener])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_connector, PeerId([4u8; 32]));
            connector.connect(target).await
        });

        let (res_listener, res_connector) = tokio::join!(handle_listener, handle_connector);
        let res_listener = res_listener.expect("listener task");
        let res_connector = res_connector.expect("connector task");

        assert!(res_listener.is_ok(), "listener failed: {res_listener:?}");
        assert!(res_connector.is_ok(), "connector failed: {res_connector:?}");
    }

    #[tokio::test]
    async fn channel_datagram_roundtrip() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let srt = Srt::new(
            RendezvousSpaceId([0xAB; 32]),
            [0x11u8; 32],
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

        let addr_listener: SocketAddr = "127.0.0.1:40201".parse().unwrap();
        let addr_connector: SocketAddr = "127.0.0.1:40202".parse().unwrap();

        let srt_listener = srt.clone();
        let srt_connector = srt.clone();

        let handle_listener = tokio::spawn(async move {
            let listener = RndzvListener::new(RendezvousSpaceId([0xAB; 32]), PeerId([3u8; 32]))
                .with_srt(srt_listener)
                .with_local_bind(addr_listener)
                .with_timeout(Duration::from_secs(3));
            let session = listener.accept().await?;
            let channel = session.open_channel(ChannelKind::UnreliableDatagram).await?;
            loop {
                if let Some(data) = channel.recv().await? {
                    return Ok::<_, RndzvError>(data);
                }
            }
        });

        let handle_connector = tokio::spawn(async move {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_connector)
                .with_remote_addrs(vec![addr_listener])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_connector, PeerId([4u8; 32]));
            let session = connector.connect(target).await?;
            let channel = session.open_channel(ChannelKind::UnreliableDatagram).await?;
            channel.send(b"hello").await
        });

        let (recv, send) = tokio::join!(handle_listener, handle_connector);
        let recv = recv.expect("listener task");
        let send = send.expect("connector task");

        assert!(send.is_ok(), "send failed: {send:?}");
        let payload = recv.expect("recv ok");
        assert_eq!(payload, b"hello");
    }

    #[tokio::test]
    async fn shutdown_completes() -> Result<(), RndzvError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let srt = Srt::new(
            RendezvousSpaceId([0xCD; 32]),
            [0x22u8; 32],
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

        let addr_listener: SocketAddr = "127.0.0.1:40301".parse().unwrap();
        let addr_connector: SocketAddr = "127.0.0.1:40302".parse().unwrap();

        let srt_listener = srt.clone();
        let srt_connector = srt.clone();

        let listener_task = tokio::spawn(async move {
            let listener = RndzvListener::new(RendezvousSpaceId([0xCD; 32]), PeerId([9u8; 32]))
                .with_srt(srt_listener)
                .with_local_bind(addr_listener)
                .with_timeout(Duration::from_secs(3));
            let session = listener.accept().await?;
            let channel = session.open_channel(ChannelKind::UnreliableDatagram).await?;
            Ok::<_, RndzvError>((session, channel))
        });

        let connector_task = tokio::spawn(async move {
            let connector = RndzvConnector::new()
                .with_local_bind(addr_connector)
                .with_remote_addrs(vec![addr_listener])
                .with_timeout(Duration::from_secs(3));
            let target = RndzvConnectTarget::from_srt(srt_connector, PeerId([8u8; 32]));
            let session = connector.connect(target).await?;
            let channel = session.open_channel(ChannelKind::UnreliableDatagram).await?;
            Ok::<_, RndzvError>((session, channel))
        });

        let (listener_res, connector_res) = tokio::join!(listener_task, connector_task);
        let (listener_session, listener_channel) = listener_res.expect("listener task")?;
        let (connector_session, connector_channel) = connector_res.expect("connector task")?;

        connector_channel.send(b"ping").await?;
        let msg = listener_channel.recv().await?.expect("listener recv");
        assert_eq!(msg, b"ping");

        listener_channel.send(b"pong").await?;
        let msg = connector_channel.recv().await?.expect("connector recv");
        assert_eq!(msg, b"pong");

        listener_session.shutdown().await?;
        connector_session.shutdown().await?;
        Ok(())
    }
}
