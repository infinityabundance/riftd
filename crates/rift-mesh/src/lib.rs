//! Mesh routing, session handling, and peer management.
//!
//! This module is the heart of Rift's P2P system. It:
//! - manages UDP sockets (direct + TURN relays)
//! - negotiates sessions and capabilities
//! - handles routing (direct vs relayed) and ICE-lite candidates
//! - performs encryption/decryption and rate limiting
//! - emits high-level events to clients

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH, Instant};

use anyhow::{anyhow, Result};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;

use rift_core::{Identity, Invite, PeerId, MessageId};
use rift_core::e2ee::{
    derive_e2ee_shared_key, ed25519_public_from_bytes, generate_e2ee_keypair,
    public_key_from_bytes, sign_e2ee_public, verify_e2ee_public,
};
use rift_discovery::{discover_peers, start_mdns_advertisement, DiscoveryConfig, MdnsHandle};
use rift_nat::{
    attempt_hole_punch, gather_public_addrs, gather_turn_candidates, NatConfig, PeerEndpoint,
    TurnRelay,
};
use rift_protocol::{
    decode_frame, encode_frame, CallControl, CallState, Capabilities, ChatMessage, CodecId,
    ControlMessage, EncryptedPayload, FeatureFlag, GroupControl, GroupMode, IceCandidate,
    CandidateType, PeerInfo, ProtocolVersion, QosProfile, RiftFrameHeader, RiftPayload, SessionId,
    StreamKind, VoicePacket,
};
use rift_metrics as metrics;
use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit};
use aes_gcm::aead::{Aead, Payload};
use rand::RngCore;
use sha2::Digest;

#[cfg(feature = "predictive-rendezvous")]
use rift_rndzv::{
    build_probe_payload, compute_slot_params, parse_probe_payload, rendezvous_id_from_seed,
    ParsedProbe, ProbePayload, RendezvousMetrics, RendezvousOutcome, RendezvousState, Role,
    SemanticRendezvousToken,
};
#[cfg(feature = "predictive-rendezvous")]
use tokio::time::Instant as TokioInstant;

/// Maximum UDP payload size we attempt to parse.
const MAX_PACKET: usize = 2048;
/// Window used for per-peer rate limiting.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(1);
/// Max packets per second before rate limiting triggers.
const RATE_LIMIT_PKTS_PER_SEC: u32 = 1200;
/// Maximum peer count for full-mesh before switching to hybrid.
const GROUP_MESH_MAX: usize = 5;

#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Channel name for session id derivation.
    pub channel_name: String,
    /// Optional channel password.
    pub password: Option<String>,
    /// UDP listen port.
    pub listen_port: u16,
    /// Whether this node can relay for others.
    pub relay_capable: bool,
    /// QoS preferences for voice traffic.
    pub qos: QosProfile,
    /// Optional auth token for access control.
    pub auth_token: Option<Vec<u8>>,
    /// Whether auth is required for peers.
    pub require_auth: bool,
    /// Optional pre-shared E2EE key for group encryption.
    pub e2ee_key: Option<[u8; 32]>,
    /// Optional rekey interval for E2EE.
    pub rekey_interval_secs: Option<u64>,
    /// Maximum number of direct peers before relaying.
    pub max_direct_peers: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum PeerRoute {
    /// Direct UDP connection to a peer address.
    Direct { addr: SocketAddr },
    /// Relayed through another peer.
    Relayed { via: PeerId },
}

#[derive(Clone)]
enum MeshSocket {
    Udp(Arc<UdpSocket>),
    Turn(Arc<TurnRelay>),
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    /// A peer joined the mesh.
    PeerJoined(PeerId),
    /// A peer left the mesh.
    PeerLeft(PeerId),
    /// Chat message received.
    ChatReceived(ChatMessage),
    /// Voice packet received.
    VoiceFrame {
        from: PeerId,
        seq: u32,
        timestamp: u64,
        session: SessionId,
        codec: CodecId,
        payload: Vec<u8>,
    },
    /// Routing info updated for a peer.
    RouteUpdated { peer_id: PeerId, route: PeerRoute },
    /// Route upgraded from relay to direct.
    RouteUpgraded(PeerId),
    /// Peer capability advertisement received.
    PeerCapabilities { peer_id: PeerId, capabilities: Capabilities },
    /// Peer session configuration received.
    PeerSessionConfig { peer_id: PeerId, codec: CodecId, frame_ms: u16 },
    GroupCodec(CodecId),
    StatsUpdate { peer: PeerId, stats: LinkStats, global: GlobalStats },
    PeerIdentity { peer_id: PeerId, public_key: Vec<u8> },
    IncomingCall {
        session: SessionId,
        from: PeerId,
        rndzv_srt_uri: Option<String>,
    },
    CallAccepted { session: SessionId, from: PeerId },
    CallDeclined {
        session: SessionId,
        from: PeerId,
        reason: Option<String>,
    },
    CallEnded { session: SessionId },
    GroupTopology { session: SessionId, mode: GroupMode },
}

#[derive(Debug, Clone, Copy)]
pub struct LinkStats {
    /// Round-trip time in milliseconds.
    pub rtt_ms: f32,
    /// Loss fraction (0.0 to 1.0).
    pub loss: f32,
    /// Jitter in milliseconds.
    pub jitter_ms: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalStats {
    /// Number of peers currently connected.
    pub num_peers: usize,
    /// Number of active sessions.
    pub num_sessions: usize,
    /// Total packets sent.
    pub packets_sent: u64,
    /// Total packets received.
    pub packets_received: u64,
    /// Total bytes sent.
    pub bytes_sent: u64,
    /// Total bytes received.
    pub bytes_received: u64,
}

/// High-level mesh controller used by clients.
pub struct Mesh {
    inner: Arc<MeshInner>,
    events_rx: mpsc::Receiver<MeshEvent>,
    discovery_config: DiscoveryConfig,
    _mdns: Option<MdnsHandle>,
}

/// Cloneable handle for sending commands without owning the event stream.
#[derive(Clone)]
pub struct MeshHandle {
    inner: Arc<MeshInner>,
}

/// Internal state shared across mesh tasks.
///
/// Many fields are guarded by mutexes because the mesh runs in multiple async
/// tasks (receiver, timers, discovery). The fields are grouped roughly by:
/// - sockets and relay management
/// - peer metadata and capabilities
/// - routing, ICE candidates, and NAT state
/// - traffic stats, rate limiting, and sessions
struct MeshInner {
    sockets: Mutex<Vec<MeshSocket>>,
    turn_relays: Mutex<HashMap<SocketAddr, usize>>,
    identity: Identity,
    peers_by_id: Mutex<HashMap<PeerId, SocketAddr>>,
    peer_caps: Mutex<HashMap<PeerId, bool>>,
    peer_capabilities: Mutex<HashMap<PeerId, Capabilities>>,
    peer_public_keys: Mutex<HashMap<PeerId, Vec<u8>>>,
    peer_session: Mutex<HashMap<PeerId, SessionConfig>>,
    preferred_codecs: Mutex<Vec<CodecId>>,
    preferred_features: Mutex<Vec<FeatureFlag>>,
    routes: Mutex<HashMap<PeerId, PeerRoute>>,
    peer_addrs: Mutex<HashMap<PeerId, SocketAddr>>,
    peer_candidates: Mutex<HashMap<PeerId, Vec<SocketAddr>>>,
    peer_ice_candidates: Mutex<HashMap<PeerId, Vec<IceCandidate>>>,
    self_candidates: Mutex<Vec<SocketAddr>>,
    self_ice_candidates: Mutex<Vec<IceCandidate>>,
    relay_candidates: Mutex<HashMap<PeerId, PeerId>>,
    connections: Mutex<HashMap<SocketAddr, PeerConnection>>,
    pending: Mutex<HashMap<PendingKey, PendingHandshake>>,
    cache: Mutex<HashSet<MessageId>>,
    voice_seq: Mutex<HashMap<PeerId, u32>>,
    control_seq: Mutex<u32>,
    nat_cfg: Mutex<Option<NatConfig>>,
    qos: QosProfile,
    link_stats: Mutex<HashMap<PeerId, LinkStatsState>>,
    peer_traffic: Mutex<HashMap<PeerId, TrafficStats>>,
    global_traffic: Mutex<TrafficStats>,
    rate_limits: Mutex<HashMap<SocketAddr, RateLimitState>>,
    auth_required: bool,
    auth_token: Option<Vec<u8>>,
    events_tx: mpsc::Sender<MeshEvent>,
    relay_capable: bool,
    session_mgr: Mutex<SessionManager>,
    active_session: Mutex<SessionId>,
    channel_session: SessionId,
    group_codec: Mutex<CodecId>,
    group_topology: Mutex<HashMap<SessionId, GroupMode>>,
    candidate_attempts: Mutex<HashMap<PeerId, tokio::time::Instant>>,
    e2ee_key: Option<[u8; 32]>,
    e2ee_keys: Mutex<HashMap<(PeerId, SessionId), [u8; 32]>>,
    e2ee_pending: Mutex<HashMap<(PeerId, SessionId), rift_core::e2ee::E2eeKeypair>>,
    e2ee_ready: Mutex<HashMap<(PeerId, SessionId), E2eeReadyState>>,
    rekey_interval_secs: Option<u64>,
    max_direct_peers: Option<usize>,
    #[cfg(feature = "predictive-rendezvous")]
    pr_sessions: Mutex<HashMap<u64, PrSession>>,
}

#[cfg(feature = "predictive-rendezvous")]
struct PrSession {
    tx: mpsc::Sender<(SocketAddr, ParsedProbe)>,
}

#[derive(Debug, Default, Clone)]
struct TrafficStats {
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

#[derive(Debug, Clone, Copy)]
struct E2eeReadyState {
    local_ready: bool,
    remote_ready: bool,
}

#[derive(Debug, Clone)]
struct RateLimitState {
    window_start: Instant,
    count: u32,
    last_drop: Instant,
}

impl RateLimitState {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            count: 0,
            last_drop: now,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= RATE_LIMIT_WINDOW {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= RATE_LIMIT_PKTS_PER_SEC {
            return false;
        }
        self.count = self.count.saturating_add(1);
        true
    }
}

#[derive(Debug, Clone)]
/// Negotiated session configuration with a peer.
pub struct SessionConfig {
    /// Selected codec.
    pub codec: CodecId,
    /// Frame duration in milliseconds.
    pub frame_ms: u16,
}

struct PeerConnection {
    peer_id: Option<PeerId>,
    session: rift_core::noise::NoiseSession,
    socket_idx: usize,
    authenticated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PendingKey {
    socket_idx: usize,
    addr: SocketAddr,
}

enum PendingHandshake {
    InitiatorAwait2(snow::HandshakeState),
    ResponderAwait3(snow::HandshakeState),
}

#[derive(Debug)]
struct SessionManager {
    sessions: HashMap<SessionId, SessionState>,
}

#[derive(Debug, Clone)]
struct SessionState {
    state: CallState,
    participants: HashSet<PeerId>,
}

#[derive(Debug, Clone)]
struct LinkStatsState {
    last_seq: Option<u32>,
    window: VecDeque<bool>,
    window_size: usize,
    last_transit_ms: Option<f32>,
    jitter_ms: f32,
    rtt_ms: f32,
    last_emit: tokio::time::Instant,
}

impl LinkStatsState {
    fn new() -> Self {
        Self {
            last_seq: None,
            window: VecDeque::with_capacity(128),
            window_size: 100,
            last_transit_ms: None,
            jitter_ms: 0.0,
            rtt_ms: 0.0,
            last_emit: tokio::time::Instant::now() - Duration::from_secs(10),
        }
    }

    fn update_on_receive(&mut self, seq: u32, sent_ms: u64, arrival_ms: u64) {
        if let Some(last) = self.last_seq {
            if seq <= last {
                return;
            }
            let gap = (seq - last).saturating_sub(1);
            let add = gap.min(self.window_size as u32) as usize;
            for _ in 0..add {
                self.push_window(false);
            }
        }
        self.push_window(true);
        self.last_seq = Some(seq);

        let transit = arrival_ms as f32 - sent_ms as f32;
        if let Some(prev) = self.last_transit_ms {
            let d = (transit - prev).abs();
            self.jitter_ms += (d - self.jitter_ms) / 16.0;
        }
        self.last_transit_ms = Some(transit);
    }

    fn update_rtt(&mut self, rtt_ms: f32) {
        if self.rtt_ms == 0.0 {
            self.rtt_ms = rtt_ms;
        } else {
            let alpha = 0.1;
            self.rtt_ms = self.rtt_ms * (1.0 - alpha) + rtt_ms * alpha;
        }
    }

    fn push_window(&mut self, received: bool) {
        if self.window.len() >= self.window_size {
            self.window.pop_front();
        }
        self.window.push_back(received);
    }

    fn loss_ratio(&self) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }
        let received = self.window.iter().filter(|v| **v).count() as f32;
        let total = self.window.len() as f32;
        (total - received) / total
    }

    fn snapshot(&self) -> LinkStats {
        LinkStats {
            rtt_ms: self.rtt_ms,
            loss: self.loss_ratio(),
            jitter_ms: self.jitter_ms,
        }
    }
}

impl SessionManager {
    fn new(channel_session: SessionId) -> Self {
        let mut sessions = HashMap::new();
        sessions.insert(
            channel_session,
            SessionState {
                state: CallState::Active,
                participants: HashSet::new(),
            },
        );
        Self { sessions }
    }

    fn ensure_session(&mut self, session: SessionId) -> &mut SessionState {
        self.sessions.entry(session).or_insert_with(|| SessionState {
            state: CallState::Ringing,
            participants: HashSet::new(),
        })
    }

    fn add_participant(&mut self, session: SessionId, peer_id: PeerId) {
        let state = self.ensure_session(session);
        state.participants.insert(peer_id);
    }

    fn remove_participant(&mut self, session: SessionId, peer_id: PeerId) {
        if let Some(state) = self.sessions.get_mut(&session) {
            state.participants.remove(&peer_id);
        }
    }

    fn remove_participant_all(&mut self, peer_id: PeerId) {
        for state in self.sessions.values_mut() {
            state.participants.remove(&peer_id);
        }
    }

    fn set_state(&mut self, session: SessionId, state: CallState) {
        let entry = self.ensure_session(session);
        entry.state = state;
    }

    fn participants(&self, session: SessionId) -> Vec<PeerId> {
        self.sessions
            .get(&session)
            .map(|state| state.participants.iter().copied().collect())
            .unwrap_or_default()
    }
}

impl Mesh {
    /// Create a new mesh instance bound to the configured UDP port.
    /// This spawns background tasks for receiving, candidate checks, rekeying,
    /// scaling decisions, and ping-based link stats.
    pub async fn new(identity: Identity, config: MeshConfig) -> Result<Self> {
        let addr = SocketAddr::from(([0, 0, 0, 0], config.listen_port));
        let socket = UdpSocket::bind(addr).await?;
        let socket = Arc::new(socket);
        let peer_id = identity.peer_id;
        let channel_session = SessionId::from_channel(&config.channel_name, config.password.as_deref());
        let session_mgr = SessionManager::new(channel_session);

        let (events_tx, events_rx) = mpsc::channel(256);
        let inner = Arc::new(MeshInner {
            sockets: Mutex::new(vec![MeshSocket::Udp(socket.clone())]),
            turn_relays: Mutex::new(HashMap::new()),
            identity,
            peers_by_id: Mutex::new(HashMap::new()),
            peer_caps: Mutex::new(HashMap::new()),
            peer_capabilities: Mutex::new(HashMap::new()),
            peer_public_keys: Mutex::new(HashMap::new()),
            peer_session: Mutex::new(HashMap::new()),
            preferred_codecs: Mutex::new(Vec::new()),
            preferred_features: Mutex::new(Vec::new()),
            routes: Mutex::new(HashMap::new()),
            group_codec: Mutex::new(CodecId::Opus),
            peer_addrs: Mutex::new(HashMap::new()),
            peer_candidates: Mutex::new(HashMap::new()),
            peer_ice_candidates: Mutex::new(HashMap::new()),
            self_candidates: Mutex::new(Vec::new()),
            self_ice_candidates: Mutex::new(Vec::new()),
            relay_candidates: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashSet::new()),
            voice_seq: Mutex::new(HashMap::new()),
            control_seq: Mutex::new(0),
            nat_cfg: Mutex::new(None),
            qos: config.qos,
            link_stats: Mutex::new(HashMap::new()),
            peer_traffic: Mutex::new(HashMap::new()),
            global_traffic: Mutex::new(TrafficStats::default()),
            rate_limits: Mutex::new(HashMap::new()),
            auth_required: config.require_auth,
            auth_token: config.auth_token,
            events_tx,
            relay_capable: config.relay_capable,
            session_mgr: Mutex::new(session_mgr),
            active_session: Mutex::new(channel_session),
            channel_session,
            candidate_attempts: Mutex::new(HashMap::new()),
            e2ee_key: config.e2ee_key,
            e2ee_keys: Mutex::new(HashMap::new()),
            e2ee_pending: Mutex::new(HashMap::new()),
            e2ee_ready: Mutex::new(HashMap::new()),
            rekey_interval_secs: config.rekey_interval_secs,
            max_direct_peers: config.max_direct_peers,
            group_topology: Mutex::new(HashMap::new()),
            #[cfg(feature = "predictive-rendezvous")]
            pr_sessions: Mutex::new(HashMap::new()),
        });

        {
            let mut topo = inner.group_topology.lock().await;
            topo.insert(channel_session, GroupMode::Mesh);
        }

        MeshInner::spawn_receiver(inner.clone(), 0, MeshSocket::Udp(socket.clone()));
        MeshInner::spawn_auto_upgrade(inner.clone());
        MeshInner::spawn_candidate_checks(inner.clone());
        MeshInner::spawn_rekey(inner.clone());
        MeshInner::spawn_scaling(inner.clone());
        MeshInner::spawn_pinger(inner.clone());

        let mesh = Self {
            inner,
            events_rx,
            discovery_config: DiscoveryConfig {
                channel_name: config.channel_name,
                password: config.password,
                peer_id,
                listen_port: socket.local_addr()?.port(),
            },
            _mdns: None,
        };

        Ok(mesh)
    }

    /// Return the local peer id.
    pub fn local_peer_id(&self) -> PeerId {
        self.inner.identity.peer_id
    }

    /// Return the local UDP listen address.
    pub async fn local_addr(&self) -> Result<SocketAddr> {
        let sockets = self.inner.sockets.lock().await;
        match sockets.first() {
            Some(MeshSocket::Udp(sock)) => {
                let addr = sock.local_addr()?;
                // Convert 0.0.0.0 to 127.0.0.1 for local connections
                if addr.ip().is_unspecified() {
                    Ok(SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                        addr.port(),
                    ))
                } else {
                    Ok(addr)
                }
            }
            Some(MeshSocket::Turn(relay)) => Ok(relay.relay_addr()),
            None => Err(anyhow!("missing socket")),
        }
    }

    /// Start LAN discovery via mDNS and spawn the discovery task.
    pub fn start_lan_discovery(&mut self) -> Result<()> {
        let mdns = start_mdns_advertisement(self.discovery_config.clone())?;
        self._mdns = Some(mdns);
        MeshInner::spawn_discovery(self.inner.clone(), self.discovery_config.clone());
        Ok(())
    }

    /// Enable NAT traversal (STUN + TURN) and populate local candidates.
    pub async fn enable_nat(&mut self, nat_cfg: NatConfig) {
        let mut cfg = self.inner.nat_cfg.lock().await;
        *cfg = Some(nat_cfg);
        drop(cfg);
        let nat_cfg = { self.inner.nat_cfg.lock().await.clone() };
        if let Some(nat_cfg) = nat_cfg {
            let local_addr = {
                let sockets = self.inner.sockets.lock().await;
                sockets.first().and_then(|sock| match sock {
                    MeshSocket::Udp(udp) => udp.local_addr().ok(),
                    MeshSocket::Turn(relay) => Some(relay.relay_addr()),
                })
            };
            let mut relay_addrs = Vec::new();
            if !nat_cfg.turn_servers.is_empty() {
                if let Ok(relays) = gather_turn_candidates(&nat_cfg).await {
                    for relay in relays {
                        let relay_addr = relay.relay_addr;
                        let relay_handle = relay.relay.clone();
                        if let Ok(socket_idx) = self.inner.add_turn_relay(relay_handle).await {
                            let mut turn_relays = self.inner.turn_relays.lock().await;
                            turn_relays.insert(relay_addr, socket_idx);
                            relay_addrs.push(relay_addr);
                            let _ = rift_nat::spawn_turn_keepalive(
                                relay.relay.clone(),
                                nat_cfg.turn_keepalive_ms,
                            );
                        }
                    }
                }
            }
            if let Ok(addrs) = gather_public_addrs(&nat_cfg).await {
                let local_addr = {
                    let sockets = self.inner.sockets.lock().await;
                    sockets.first().and_then(|sock| match sock {
                        MeshSocket::Udp(udp) => udp.local_addr().ok(),
                        MeshSocket::Turn(relay) => Some(relay.relay_addr()),
                    })
                };
                let mut combined = addrs.clone();
                combined.extend(relay_addrs.clone());
                if let Some(local) = local_addr {
                    combined.push(local);
                }
                combined.sort();
                combined.dedup();
                let mut self_candidates = self.inner.self_candidates.lock().await;
                *self_candidates = combined;
                drop(self_candidates);
                let ice_candidates = MeshInner::build_ice_candidates(local_addr, &addrs, &relay_addrs);
                let mut self_ice = self.inner.self_ice_candidates.lock().await;
                *self_ice = ice_candidates;
            } else if let Some(local) = local_addr {
                let mut self_candidates = self.inner.self_candidates.lock().await;
                let mut combined = vec![local];
                combined.extend(relay_addrs.clone());
                combined.sort();
                combined.dedup();
                *self_candidates = combined;
                drop(self_candidates);
                let ice_candidates = MeshInner::build_ice_candidates(Some(local), &[], &relay_addrs);
                let mut self_ice = self.inner.self_ice_candidates.lock().await;
                *self_ice = ice_candidates;
            }
        }
    }

    /// Join peers from an invite using NAT traversal for connectivity.
    pub async fn join_invite(&mut self, invite: Invite, nat_cfg: NatConfig) -> Result<()> {
        {
            let mut cfg = self.inner.nat_cfg.lock().await;
            *cfg = Some(nat_cfg.clone());
        }

        let mut targets = invite.known_peers;
        targets.extend(invite.candidates);
        targets.sort();
        targets.dedup();

        let mut candidates: Vec<IceCandidate> = targets
            .into_iter()
            .map(|addr| IceCandidate {
                addr,
                cand_type: CandidateType::Srflx,
                priority: 50,
                foundation: MeshInner::candidate_foundation(addr, CandidateType::Srflx),
            })
            .collect();
        for relay_addr in invite.relay_candidates {
            candidates.push(IceCandidate {
                addr: relay_addr,
                cand_type: CandidateType::Relay,
                priority: 70,
                foundation: MeshInner::candidate_foundation(relay_addr, CandidateType::Relay),
            });
        }

        for candidate in candidates {
            match candidate.cand_type {
                CandidateType::Relay => {
                    let socket_idx = {
                        let map = self.inner.turn_relays.lock().await;
                        map.get(&candidate.addr).copied()
                    };
                    if let Some(socket_idx) = socket_idx {
                        if let Err(err) = self.inner.initiate_handshake(candidate.addr, socket_idx).await {
                            tracing::warn!("handshake to {} failed: {err}", candidate.addr);
                        }
                    }
                }
                _ => {
                    // Try direct handshake on the primary socket first (useful for LAN/local invites).
                    if let Err(err) = self.inner.initiate_handshake(candidate.addr, 0).await {
                        tracing::warn!("direct handshake to {} failed: {err}", candidate.addr);
                    } else {
                        continue;
                    }

                    let endpoint = PeerEndpoint {
                        peer_id: PeerId([0u8; 32]),
                        external_addrs: vec![candidate.addr],
                        punch_ports: vec![candidate.addr.port()],
                    };
                    if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                        let socket_idx = self.inner.add_socket(socket).await?;
                        if let Err(err) = self.inner.initiate_handshake(remote, socket_idx).await {
                            tracing::warn!("handshake to {} failed: {err}", remote);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Return the currently active session id.
    pub async fn active_session(&self) -> SessionId {
        *self.inner.active_session.lock().await
    }

    /// Await the next mesh event.
    pub async fn next_event(&mut self) -> Option<MeshEvent> {
        self.events_rx.recv().await
    }

    /// Obtain a cloneable handle for issuing commands.
    pub fn handle(&self) -> MeshHandle {
        MeshHandle {
            inner: self.inner.clone(),
        }
    }
}

impl MeshInner {
    fn build_ice_candidates(
        local_addr: Option<SocketAddr>,
        public_addrs: &[SocketAddr],
        relay_addrs: &[SocketAddr],
    ) -> Vec<IceCandidate> {
        let mut out = Vec::new();
        if let Some(addr) = local_addr {
            out.push(IceCandidate {
                addr,
                cand_type: CandidateType::Host,
                priority: 100,
                foundation: Self::candidate_foundation(addr, CandidateType::Host),
            });
        }
        for addr in public_addrs {
            out.push(IceCandidate {
                addr: *addr,
                cand_type: CandidateType::Srflx,
                priority: 90,
                foundation: Self::candidate_foundation(*addr, CandidateType::Srflx),
            });
        }
        for addr in relay_addrs {
            out.push(IceCandidate {
                addr: *addr,
                cand_type: CandidateType::Relay,
                priority: 70,
                foundation: Self::candidate_foundation(*addr, CandidateType::Relay),
            });
        }
        out.sort_by(|a, b| b.priority.cmp(&a.priority));
        out.dedup_by(|a, b| a.addr == b.addr && a.cand_type == b.cand_type);
        out
    }

    fn candidate_foundation(addr: SocketAddr, cand_type: CandidateType) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        addr.hash(&mut hasher);
        cand_type.hash(&mut hasher);
        hasher.finish()
    }

    async fn current_group_mode(&self, session: SessionId) -> GroupMode {
        let topo = self.group_topology.lock().await;
        topo.get(&session).cloned().unwrap_or(GroupMode::Mesh)
    }

    async fn set_group_mode(&self, session: SessionId, mode: GroupMode) {
        let mut topo = self.group_topology.lock().await;
        topo.insert(session, mode);
    }

    async fn is_topology_coordinator(&self, session: SessionId) -> bool {
        let peers = {
            let mgr = self.session_mgr.lock().await;
            mgr.participants(session)
        };
        if peers.is_empty() {
            return true;
        }
        let mut all = peers;
        if !all.contains(&self.identity.peer_id) {
            all.push(self.identity.peer_id);
        }
        all.sort_by_key(|p| p.0);
        all.first().copied() == Some(self.identity.peer_id)
    }

    async fn select_forwarder(&self, session: SessionId) -> Option<PeerId> {
        let participants = {
            let mgr = self.session_mgr.lock().await;
            mgr.participants(session)
        };
        if participants.is_empty() {
            return None;
        }
        let relay_caps = self.peer_caps.lock().await.clone();
        let stats = self.link_stats.lock().await.clone();

        let mut candidates: Vec<PeerId> = participants
            .into_iter()
            .filter(|peer| relay_caps.get(peer).copied().unwrap_or(false) || *peer == self.identity.peer_id)
            .collect();
        if self.relay_capable && !candidates.contains(&self.identity.peer_id) {
            candidates.push(self.identity.peer_id);
        }
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by_key(|peer| {
            if *peer == self.identity.peer_id {
                0u64
            } else if let Some(st) = stats.get(peer) {
                (st.rtt_ms as u64).saturating_add((st.loss_ratio() * 1000.0) as u64)
            } else {
                10_000
            }
        });
        candidates.first().copied()
    }

    async fn update_group_topology(&self, session: SessionId) -> Result<()> {
        if !self.is_topology_coordinator(session).await {
            return Ok(());
        }
        let participants = {
            let mgr = self.session_mgr.lock().await;
            mgr.participants(session)
        };
        // Include local peer in count (participants only contains remote peers)
        let count = participants.len() + 1;
        let next = if count <= GROUP_MESH_MAX {
            GroupMode::Mesh
        } else if self.e2ee_key.is_some() {
            if let Some(forwarder) = self.select_forwarder(session).await {
                GroupMode::Hybrid { forwarder }
            } else {
                GroupMode::Mesh
            }
        } else {
            GroupMode::Mesh
        };

        let current = self.current_group_mode(session).await;
        if current == next {
            return Ok(());
        }
        self.set_group_mode(session, next).await;
        let msg = ControlMessage::Group(GroupControl::Topology { session, mode: next });
        for peer in participants {
            if peer == self.identity.peer_id {
                continue;
            }
            let _ = self.send_control_to_peer(peer, msg.clone(), session).await;
        }
        let _ = self
            .events_tx
            .send(MeshEvent::GroupTopology { session, mode: next })
            .await;
        Ok(())
    }

    async fn should_forward_voice(&self, session: SessionId, from: PeerId) -> bool {
        if from == self.identity.peer_id {
            return false;
        }
        match self.current_group_mode(session).await {
            GroupMode::Hybrid { forwarder } if forwarder == self.identity.peer_id => true,
            _ => false,
        }
    }

    async fn should_use_group_key(&self, session: SessionId) -> bool {
        matches!(self.current_group_mode(session).await, GroupMode::Hybrid { .. }) && self.e2ee_key.is_some()
    }
    async fn broadcast_chat(&self, text: String) -> Result<()> {
        let timestamp = now_timestamp();
        let seq = self.next_control_seq().await;
        let chat = ChatMessage::new(self.identity.peer_id, timestamp, text);
        let payload = RiftPayload::Control(ControlMessage::Chat(chat.clone()));
        let session = self.channel_session;

        let mut cache = self.cache.lock().await;
        cache.insert(chat.id);
        drop(cache);

        let routes = self.routes_snapshot().await;
        for (peer_id, route) in routes {
            if peer_id == self.identity.peer_id {
                continue;
            }
            if let Err(err) = self
                .send_to_peer(peer_id, route, payload.clone(), seq, timestamp, session)
                .await
            {
                tracing::debug!(peer = %peer_id, "chat send failed: {err}");
            }
        }

        Ok(())
    }
    fn spawn_receiver(inner: Arc<Self>, socket_idx: usize, socket: MeshSocket) {
        tokio::spawn(async move {
            let mut buf = [0u8; MAX_PACKET];
            match socket {
                MeshSocket::Udp(socket) => loop {
                    let (len, addr) = match socket.recv_from(&mut buf).await {
                        Ok(res) => res,
                        Err(_) => continue,
                    };
                    if let Err(err) = inner
                        .clone()
                        .handle_packet(socket_idx, addr, &buf[..len])
                        .await
                    {
                        tracing::warn!("mesh recv error: {err}");
                    }
                },
                MeshSocket::Turn(relay) => loop {
                    let res = relay.recv_from(&mut buf).await;
                    let Ok((len, addr)) = res else {
                        tracing::warn!("turn recv error");
                        continue;
                    };
                    if let Err(err) = inner
                        .clone()
                        .handle_packet(socket_idx, addr, &buf[..len])
                        .await
                    {
                        tracing::warn!("mesh recv error: {err}");
                    }
                },
            }
        });
    }

    fn spawn_discovery(inner: Arc<Self>, config: DiscoveryConfig) {
        tokio::spawn(async move {
            loop {
                let stream = match discover_peers(config.clone()) {
                    Ok(stream) => stream,
                    Err(err) => {
                        tracing::warn!("discovery failed: {err}");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };
                tokio::pin!(stream);
                let mut window = tokio::time::interval(Duration::from_millis(200));
                let window_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
                loop {
                    tokio::select! {
                        maybe = stream.next() => {
                            let Some(peer) = maybe else { break; };
                            if peer.peer_id == inner.identity.peer_id {
                                continue;
                            }
                            let already = {
                                let connections = inner.connections.lock().await;
                                connections.contains_key(&peer.addr)
                            };
                            if already {
                                continue;
                            }
                            if let Err(err) = inner.initiate_handshake(peer.addr, 0).await {
                                tracing::warn!("handshake to {} failed: {err}", peer.addr);
                                let inner_retry = inner.clone();
                                tokio::spawn(async move {
                                    for attempt in 1..=10 {
                                        tokio::time::sleep(Duration::from_secs(1)).await;
                                        let already = {
                                            let connections = inner_retry.connections.lock().await;
                                            connections.contains_key(&peer.addr)
                                        };
                                        if already {
                                            break;
                                        }
                                        if let Err(err) = inner_retry.initiate_handshake(peer.addr, 0).await {
                                            tracing::debug!(
                                                "handshake retry {} to {} failed: {err}",
                                                attempt,
                                                peer.addr
                                            );
                                        } else {
                                            break;
                                        }
                                    }
                                });
                            }
                        }
                        _ = window.tick() => {
                            if tokio::time::Instant::now() >= window_deadline {
                                break;
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }

    fn spawn_auto_upgrade(inner: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                let nat_cfg = { inner.nat_cfg.lock().await.clone() };
                let Some(nat_cfg) = nat_cfg else {
                    continue;
                };
                let relayed: Vec<(PeerId, Vec<SocketAddr>)> = {
                    let routes = inner.routes.lock().await;
                    let addrs = inner.peer_addrs.lock().await;
                    let candidates = inner.peer_candidates.lock().await;
                    routes
                        .iter()
                        .filter_map(|(peer_id, route)| match route {
                            PeerRoute::Relayed { .. } => {
                                if let Some(cands) = candidates.get(peer_id) {
                                    Some((*peer_id, cands.clone()))
                                } else {
                                    addrs
                                        .get(peer_id)
                                        .copied()
                                        .map(|addr| (*peer_id, vec![addr]))
                                }
                            }
                            _ => None,
                        })
                        .collect()
                };

                for (peer_id, addrs) in relayed {
                    let endpoint = PeerEndpoint {
                        peer_id,
                        external_addrs: addrs.clone(),
                        punch_ports: addrs.iter().map(|addr| addr.port()).collect(),
                    };
                    if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                        if let Ok(socket_idx) = inner.add_socket(socket).await {
                            if let Err(err) = inner.initiate_handshake(remote, socket_idx).await {
                                tracing::debug!(
                                    peer = %peer_id,
                                    "auto-upgrade handshake failed: {err}"
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    fn spawn_candidate_checks(inner: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            loop {
                tick.tick().await;
                let nat_cfg = { inner.nat_cfg.lock().await.clone() };
                let Some(nat_cfg) = nat_cfg else {
                    continue;
                };
                let ice_map = { inner.peer_ice_candidates.lock().await.clone() };
                let candidates_map = { inner.peer_candidates.lock().await.clone() };
                let routes = { inner.routes.lock().await.clone() };
                let addrs = { inner.peer_addrs.lock().await.clone() };
                let now = tokio::time::Instant::now();

                let mut targets: Vec<(PeerId, Vec<IceCandidate>)> = Vec::new();
                for (peer_id, route) in routes.iter() {
                    if matches!(route, PeerRoute::Direct { .. }) {
                        continue;
                    }
                    let candidates = if let Some(ice) = ice_map.get(peer_id) {
                        let mut sorted = ice.clone();
                        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));
                        sorted
                    } else {
                        candidates_map
                            .get(peer_id)
                            .cloned()
                            .or_else(|| addrs.get(peer_id).copied().map(|addr| vec![addr]))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|addr| IceCandidate {
                                addr,
                                cand_type: CandidateType::Srflx,
                                priority: 50,
                                foundation: MeshInner::candidate_foundation(addr, CandidateType::Srflx),
                            })
                            .collect()
                    };
                    if candidates.is_empty() {
                        continue;
                    }
                    targets.push((*peer_id, candidates));
                }

                for (peer_id, candidates) in targets {
                    let mut attempts = inner.candidate_attempts.lock().await;
                    if let Some(last) = attempts.get(&peer_id) {
                        if now.duration_since(*last) < Duration::from_secs(20) {
                            continue;
                        }
                    }
                    attempts.insert(peer_id, now);
                    drop(attempts);

                    for candidate in candidates {
                        let mut connected = None;
                        match candidate.cand_type {
                            CandidateType::Relay => {
                                let socket_idx = {
                                    let map = inner.turn_relays.lock().await;
                                    map.get(&candidate.addr).copied()
                                };
                                if let Some(socket_idx) = socket_idx {
                                    if let Err(err) = inner.initiate_handshake(candidate.addr, socket_idx).await {
                                        tracing::debug!(
                                            peer = %peer_id,
                                            "turn candidate handshake failed: {err}"
                                        );
                                    } else {
                                        connected = Some(candidate.addr);
                                    }
                                }
                            }
                            _ => {
                                let endpoint = PeerEndpoint {
                                    peer_id,
                                    external_addrs: vec![candidate.addr],
                                    punch_ports: vec![candidate.addr.port()],
                                };
                                if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                                    if let Ok(socket_idx) = inner.add_socket(socket).await {
                                        if let Err(err) = inner.initiate_handshake(remote, socket_idx).await {
                                            tracing::debug!(
                                                peer = %peer_id,
                                                "candidate check handshake failed: {err}"
                                            );
                                        } else {
                                            connected = Some(remote);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(remote) = connected {
                            let msg = ControlMessage::IceCheck {
                                session: SessionId::NONE,
                                tie_breaker: rand::random::<u64>(),
                                candidate: candidate.clone(),
                            };
                            let _ = inner.send_control_to_peer(peer_id, msg, SessionId::NONE).await;
                            if remote == candidate.addr {
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    fn spawn_pinger(inner: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            let mut nonce: u64 = 1;
            loop {
                tick.tick().await;
                let peers: Vec<PeerId> = {
                    let routes = inner.routes.lock().await;
                    routes.keys().copied().collect()
                };
                for peer_id in peers {
                    if peer_id == inner.identity.peer_id {
                        continue;
                    }
                    let ping = ControlMessage::Ping {
                        nonce,
                        sent_at_ms: now_timestamp(),
                    };
                    nonce = nonce.wrapping_add(1);
                    let _ = inner
                        .send_control_to_peer(peer_id, ping, SessionId::NONE)
                        .await;
                }
            }
        });
    }

    fn spawn_rekey(inner: Arc<Self>) {
        tokio::spawn(async move {
            let interval = inner.rekey_interval_secs.unwrap_or(0);
            if interval == 0 {
                return;
            }
            let mut tick = tokio::time::interval(Duration::from_secs(interval));
            loop {
                tick.tick().await;
                let routes = inner.routes.lock().await.clone();
                let peers = inner.peers_by_id.lock().await.clone();
                for (peer_id, route) in routes {
                    if !matches!(route, PeerRoute::Direct { .. }) {
                        continue;
                    }
                    if let Some(addr) = peers.get(&peer_id).copied() {
                        if let Err(err) = inner.initiate_handshake(addr, 0).await {
                            tracing::debug!(peer = %peer_id, "rekey handshake failed: {err}");
                        }
                    }
                }
            }
        });
    }

    fn spawn_scaling(inner: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            loop {
                tick.tick().await;
                let Some(max_direct) = inner.max_direct_peers else {
                    continue;
                };
                let routes = inner.routes.lock().await.clone();
                let relay_candidates = inner.relay_candidates.lock().await.clone();
                let peer_addrs = inner.peer_addrs.lock().await.clone();
                let mut direct_peers: Vec<PeerId> = routes
                    .iter()
                    .filter_map(|(peer_id, route)| match route {
                        PeerRoute::Direct { .. } => Some(*peer_id),
                        _ => None,
                    })
                    .collect();

                if direct_peers.len() <= max_direct {
                    continue;
                }

                direct_peers.sort_by_key(|peer| peer.to_hex());
                let mut routes_guard = inner.routes.lock().await;
                let mut direct_count = direct_peers.len();
                for peer_id in direct_peers {
                    if direct_count <= max_direct {
                        break;
                    }
                    let Some(relay) = relay_candidates.get(&peer_id).copied() else {
                        continue;
                    };
                    if matches!(routes_guard.get(&peer_id), Some(PeerRoute::Relayed { .. })) {
                        continue;
                    }
                    routes_guard.insert(peer_id, PeerRoute::Relayed { via: relay });
                    direct_count = direct_count.saturating_sub(1);
                    drop(routes_guard);
                    let _ = inner
                        .events_tx
                        .send(MeshEvent::RouteUpdated {
                            peer_id,
                            route: PeerRoute::Relayed { via: relay },
                        })
                        .await;
                    tracing::info!(peer = %peer_id, relay = %relay, "scaled route to relay");
                    routes_guard = inner.routes.lock().await;
                }

                if direct_count <= max_direct {
                    continue;
                }

                // If we still exceed max_direct and have direct routes without relay candidates,
                // keep them as-is until a relay candidate becomes available.
                drop(routes_guard);

                // If we are under the limit, attempt to restore relayed routes to direct.
                if direct_count < max_direct {
                    let mut routes_guard = inner.routes.lock().await;
                    for (peer_id, route) in routes.iter() {
                        if !matches!(route, PeerRoute::Relayed { .. }) {
                            continue;
                        }
                        if let Some(addr) = peer_addrs.get(peer_id).copied() {
                            routes_guard.insert(*peer_id, PeerRoute::Direct { addr });
                            let _ = inner
                                .events_tx
                                .send(MeshEvent::RouteUpdated {
                                    peer_id: *peer_id,
                                    route: PeerRoute::Direct { addr },
                                })
                                .await;
                            tracing::info!(peer = %peer_id, "scaled route back to direct");
                        }
                    }
                }
            }
        });
    }

    async fn add_socket(self: &Arc<Self>, socket: UdpSocket) -> Result<usize> {
        let socket = Arc::new(socket);
        let mut sockets = self.sockets.lock().await;
        sockets.push(MeshSocket::Udp(socket.clone()));
        let idx = sockets.len() - 1;
        drop(sockets);
        Self::spawn_receiver(self.clone(), idx, MeshSocket::Udp(socket));
        Ok(idx)
    }

    async fn add_turn_relay(self: &Arc<Self>, relay: Arc<TurnRelay>) -> Result<usize> {
        let mut sockets = self.sockets.lock().await;
        sockets.push(MeshSocket::Turn(relay.clone()));
        let idx = sockets.len() - 1;
        drop(sockets);
        Self::spawn_receiver(self.clone(), idx, MeshSocket::Turn(relay));
        Ok(idx)
    }

    async fn initiate_handshake(&self, addr: SocketAddr, socket_idx: usize) -> Result<()> {
        let builder = rift_core::noise::noise_builder();
        let static_kp = builder.generate_keypair()?;
        let mut hs = builder
            .local_private_key(&static_kp.private)
            .build_initiator()?;

        let mut buf = [0u8; MAX_PACKET];
        let len = hs.write_message(&[], &mut buf)?;
        self.send_raw(socket_idx, addr, &buf[..len]).await?;

        let mut pending = self.pending.lock().await;
        pending.insert(
            PendingKey { socket_idx, addr },
            PendingHandshake::InitiatorAwait2(hs),
        );
        Ok(())
    }

    async fn handle_packet(self: Arc<Self>, socket_idx: usize, addr: SocketAddr, data: &[u8]) -> Result<()> {
        if !self.allow_packet(addr).await {
            return Ok(());
        }
        #[cfg(feature = "predictive-rendezvous")]
        if self.try_handle_pr_probe(addr, data).await? {
            return Ok(());
        }
        if self.try_handle_pending(socket_idx, addr, data).await? {
            return Ok(());
        }

        let maybe_session = {
            let mut connections = self.connections.lock().await;
            connections.get_mut(&addr).map(|conn| {
                let mut out = vec![0u8; MAX_PACKET];
                let len = conn.session.decrypt(data, &mut out)?;
                Ok::<_, anyhow::Error>(out[..len].to_vec())
            })
        };

        let plaintext = match maybe_session {
            Some(res) => res?,
            None => {
                self.start_responder(socket_idx, addr, data).await?;
                return Ok(());
            }
        };

        self.record_recv(addr, data.len()).await;
        let (header, payload) = decode_frame(&plaintext)?;
        self.update_link_stats(header.source, header.seq, header.timestamp)
            .await;
        self.handle_frame(addr, header, payload).await?;
        Ok(())
    }

    async fn try_handle_pending(
        &self,
        socket_idx: usize,
        addr: SocketAddr,
        data: &[u8],
    ) -> Result<bool> {
        let mut pending = self.pending.lock().await;
        let key = PendingKey { socket_idx, addr };
        let Some(state) = pending.remove(&key) else {
            return Ok(false);
        };
        drop(pending);

        match state {
            PendingHandshake::InitiatorAwait2(mut hs) => {
                let mut out = [0u8; MAX_PACKET];
                hs.read_message(data, &mut out)?;
                let len = hs.write_message(&[], &mut out)?;
                self.send_raw(socket_idx, addr, &out[..len]).await?;
                let transport = hs.into_transport_mode()?;
                self.install_connection(addr, transport, socket_idx).await?;
            }
            PendingHandshake::ResponderAwait3(mut hs) => {
                let mut out = [0u8; MAX_PACKET];
                hs.read_message(data, &mut out)?;
                let transport = hs.into_transport_mode()?;
                self.install_connection(addr, transport, socket_idx).await?;
            }
        }

        Ok(true)
    }

    async fn start_responder(
        &self,
        socket_idx: usize,
        addr: SocketAddr,
        first_msg: &[u8],
    ) -> Result<()> {
        let builder = rift_core::noise::noise_builder();
        let static_kp = builder.generate_keypair()?;
        let mut hs = builder
            .local_private_key(&static_kp.private)
            .build_responder()?;

        let mut out = [0u8; MAX_PACKET];
        hs.read_message(first_msg, &mut out)?;
        let len = hs.write_message(&[], &mut out)?;
        self.send_raw(socket_idx, addr, &out[..len]).await?;

        let mut pending = self.pending.lock().await;
        pending.insert(
            PendingKey { socket_idx, addr },
            PendingHandshake::ResponderAwait3(hs),
        );
        Ok(())
    }

    async fn install_connection(
        &self,
        addr: SocketAddr,
        transport: snow::TransportState,
        socket_idx: usize,
    ) -> Result<()> {
        let conn = PeerConnection {
            peer_id: None,
            session: rift_core::noise::NoiseSession::new(transport),
            socket_idx,
            authenticated: !self.auth_required,
        };
        let mut connections = self.connections.lock().await;
        connections.insert(addr, conn);
        drop(connections);

        if let Some(token) = self.auth_token.clone() {
            let auth = ControlMessage::Auth { token };
            let seq = self.next_control_seq().await;
            let _ = self
                .send_payload(
                    addr,
                    RiftPayload::Control(auth),
                    seq,
                    now_timestamp(),
                    SessionId::NONE,
                    None,
                )
                .await;
        }

        let join = ControlMessage::Join {
            peer_id: self.identity.peer_id,
            display_name: None,
        };
        let seq = self.next_control_seq().await;
        self.send_payload(
            addr,
            RiftPayload::Control(join),
            seq,
            now_timestamp(),
            SessionId::NONE,
            None,
        )
        .await?;

        let state = ControlMessage::PeerState {
            peer_id: self.identity.peer_id,
            relay_capable: self.relay_capable,
        };
        let seq = self.next_control_seq().await;
        self.send_payload(
            addr,
            RiftPayload::Control(state),
            seq,
            now_timestamp(),
            SessionId::NONE,
            None,
        )
        .await?;

        self.send_peer_list(addr).await?;
        self.send_hello(addr).await?;
        Ok(())
    }

    async fn send_peer_list(&self, addr: SocketAddr) -> Result<()> {
        let peers: Vec<PeerInfo> = {
            let peers_by_id = self.peers_by_id.lock().await;
            let peer_caps = self.peer_caps.lock().await;
            let peer_candidates = self.peer_candidates.lock().await;
            peers_by_id
                .iter()
                .map(|(peer_id, addr)| {
                    let mut addrs = peer_candidates
                        .get(peer_id)
                        .cloned()
                        .unwrap_or_else(|| vec![*addr]);
                    if addrs.is_empty() {
                        addrs.push(*addr);
                    }
                    addrs.sort();
                    addrs.dedup();
                    PeerInfo {
                        peer_id: *peer_id,
                        addr: *addr,
                        addrs,
                        relay_capable: *peer_caps.get(peer_id).unwrap_or(&false),
                    }
                })
                .collect()
        };
        let msg = RiftPayload::Control(ControlMessage::PeerList { peers });
        let seq = self.next_control_seq().await;
        self.send_payload(addr, msg, seq, now_timestamp(), SessionId::NONE, None)
            .await
    }

    async fn handle_capabilities(&self, peer_id: PeerId, capabilities: Capabilities) -> Result<()> {
        {
            let mut caps = self.peer_capabilities.lock().await;
            caps.insert(peer_id, capabilities.clone());
        }

        let session = self.negotiate_session_config(&capabilities).await;
        {
            let mut sessions = self.peer_session.lock().await;
            sessions.insert(peer_id, session.clone());
        }
        self.update_group_codec().await;

        let _ = self
            .events_tx
            .send(MeshEvent::PeerCapabilities {
                peer_id,
                capabilities: capabilities.clone(),
            })
            .await;
        let _ = self
            .events_tx
            .send(MeshEvent::PeerSessionConfig {
                peer_id,
                codec: session.codec,
                frame_ms: session.frame_ms,
            })
            .await;
        self.maybe_start_e2ee(peer_id, &capabilities).await?;
        Ok(())
    }

    async fn maybe_start_e2ee(&self, peer_id: PeerId, capabilities: &Capabilities) -> Result<()> {
        if self.e2ee_key.is_some() {
            return Ok(());
        }
        let local_supports = {
            let preferred = self.preferred_features.lock().await;
            if preferred.is_empty() {
                true
            } else {
                preferred.contains(&FeatureFlag::E2EE)
            }
        };
        if !local_supports || !capabilities.features.contains(&FeatureFlag::E2EE) {
            return Ok(());
        }
        let _ = self.start_e2ee(peer_id, self.channel_session).await;
        Ok(())
    }

    async fn negotiate_session_config(&self, remote: &Capabilities) -> SessionConfig {
        let preferred = {
            let prefs = self.preferred_codecs.lock().await;
            if prefs.is_empty() {
                vec![CodecId::Opus, CodecId::PCM16]
            } else {
                prefs.clone()
            }
        };
        let codec = preferred
            .into_iter()
            .find(|codec| remote.audio_codecs.contains(codec))
            .unwrap_or(CodecId::Opus);
        let frame_ms = remote
            .preferred_frame_duration_ms
            .unwrap_or(20)
            .min(20);
        SessionConfig { codec, frame_ms }
    }

    async fn update_group_codec(&self) {
        let caps = self.peer_capabilities.lock().await;
        let preferred = {
            let prefs = self.preferred_codecs.lock().await;
            if prefs.is_empty() {
                vec![CodecId::Opus, CodecId::PCM16]
            } else {
                prefs.clone()
            }
        };
        let mut selected = CodecId::Opus;
        for codec in preferred {
            let mut all_support = true;
            for cap in caps.values() {
                if !cap.audio_codecs.contains(&codec) {
                    all_support = false;
                    break;
                }
            }
            if all_support {
                selected = codec;
                break;
            }
        }
        let mut group = self.group_codec.lock().await;
        if *group != selected {
            *group = selected;
            let _ = self.events_tx.send(MeshEvent::GroupCodec(selected)).await;
        }
    }

    async fn send_hello(&self, addr: SocketAddr) -> Result<()> {
        let caps = self.default_capabilities().await;
        let public_key = self.identity.keypair.public.to_bytes().to_vec();
        let candidates = {
            let self_candidates = self.self_candidates.lock().await;
            self_candidates.clone()
        };
        let msg = RiftPayload::Control(ControlMessage::Hello {
            peer_id: self.identity.peer_id,
            public_key,
            capabilities: caps,
            candidates,
        });
        let seq = self.next_control_seq().await;
        self.send_payload(addr, msg, seq, now_timestamp(), SessionId::NONE, None)
            .await
    }

    async fn send_ice_candidates(&self, peer_id: PeerId, session: SessionId) -> Result<()> {
        let candidates = {
            let self_ice = self.self_ice_candidates.lock().await;
            self_ice.clone()
        };
        if candidates.is_empty() {
            return Ok(());
        }
        let msg = ControlMessage::IceCandidates {
            peer_id: self.identity.peer_id,
            session,
            candidates,
        };
        self.send_control_to_peer(peer_id, msg, session).await
    }

    async fn default_capabilities(&self) -> Capabilities {
        let preferred = self.preferred_codecs().await;
        let codecs = if preferred.is_empty() {
            vec![CodecId::Opus, CodecId::PCM16]
        } else {
            preferred
        };
        let features = {
            let preferred = self.preferred_features.lock().await;
            if preferred.is_empty() {
                vec![
                    FeatureFlag::Voice,
                    FeatureFlag::Text,
                    FeatureFlag::Relay,
                    FeatureFlag::E2EE,
                ]
            } else {
                preferred.clone()
            }
        };
        Capabilities {
            supported_versions: vec![ProtocolVersion::V2, ProtocolVersion::V1],
            audio_codecs: codecs,
            features,
            max_bitrate: Some(96_000),
            preferred_frame_duration_ms: Some(20),
        }
    }

    async fn preferred_codecs(&self) -> Vec<CodecId> {
        self.preferred_codecs.lock().await.clone()
    }

    async fn set_preferred_codecs(&self, codecs: Vec<CodecId>) {
        let mut preferred = self.preferred_codecs.lock().await;
        *preferred = codecs;
    }

    async fn set_preferred_features(&self, features: Vec<FeatureFlag>) {
        let mut preferred = self.preferred_features.lock().await;
        *preferred = features;
    }

    async fn next_control_seq(&self) -> u32 {
        let mut seq = self.control_seq.lock().await;
        let current = *seq;
        *seq = seq.wrapping_add(1);
        current
    }

    async fn send_control_to_peer(
        &self,
        peer_id: PeerId,
        msg: ControlMessage,
        session: SessionId,
    ) -> Result<()> {
        let payload = RiftPayload::Control(msg);
        let routes = self.routes_snapshot().await;
        let Some(route) = routes.get(&peer_id).cloned() else {
            return Err(anyhow!("missing route"));
        };
        let seq = self.next_control_seq().await;
        self.send_to_peer(peer_id, route, payload, seq, now_timestamp(), session)
            .await
    }

    async fn start_e2ee(&self, peer_id: PeerId, session: SessionId) -> Result<()> {
        if peer_id == self.identity.peer_id {
            return Ok(());
        }
        // Deterministic initiator: only the lower peer id sends init to avoid key mismatch.
        if self.identity.peer_id.0 > peer_id.0 {
            return Ok(());
        }
        {
            let keys = self.e2ee_keys.lock().await;
            if keys.contains_key(&(peer_id, session)) {
                return Ok(());
            }
        }
        {
            let pending = self.e2ee_pending.lock().await;
            if pending.contains_key(&(peer_id, session)) {
                return Ok(());
            }
        }
        let peer_public = {
            let keys = self.peer_public_keys.lock().await;
            keys.get(&peer_id).cloned()
        };
        let Some(peer_public) = peer_public else {
            return Ok(());
        };
        let Some(_peer_public) = ed25519_public_from_bytes(&peer_public) else {
            return Ok(());
        };
        let keypair = generate_e2ee_keypair();
        let signature = sign_e2ee_public(&self.identity, &session.0, &keypair.public);
        let public_key = keypair.public.to_bytes();
        {
            let mut pending = self.e2ee_pending.lock().await;
            pending.insert((peer_id, session), keypair);
        }
        let msg = ControlMessage::E2eeInit {
            session,
            from: self.identity.peer_id,
            public_key,
            signature,
        };
        self.send_control_to_peer(peer_id, msg, session).await?;
        tracing::info!(peer = %peer_id, session = ?session, "e2ee init sent");
        Ok(())
    }

    async fn handle_e2ee_init(
        &self,
        peer_id: PeerId,
        session: SessionId,
        public_key: [u8; 32],
        signature: Vec<u8>,
    ) -> Result<()> {
        let should_initiate = self.identity.peer_id.0 < peer_id.0;
        {
            let keys = self.e2ee_keys.lock().await;
            if keys.contains_key(&(peer_id, session)) {
                return Ok(());
            }
        }
        if should_initiate {
            let pending = self.e2ee_pending.lock().await;
            if pending.contains_key(&(peer_id, session)) {
                tracing::info!(
                    peer = %peer_id,
                    session = ?session,
                    "e2ee init ignored (local is initiator)"
                );
                return Ok(());
            }
        } else {
            let mut pending = self.e2ee_pending.lock().await;
            pending.remove(&(peer_id, session));
        }
        let peer_public = {
            let keys = self.peer_public_keys.lock().await;
            keys.get(&peer_id).cloned()
        };
        let Some(peer_public) = peer_public else {
            return Ok(());
        };
        let Some(peer_public) = ed25519_public_from_bytes(&peer_public) else {
            return Ok(());
        };
        let remote_public = public_key_from_bytes(public_key);
        if !verify_e2ee_public(&peer_public, &session.0, &remote_public, &signature) {
            tracing::warn!(peer = %peer_id, "e2ee init signature invalid");
            return Ok(());
        }
        let keypair = generate_e2ee_keypair();
        let shared = derive_e2ee_shared_key(&keypair.secret, &remote_public, &session.0);
        let shared_fingerprint = hex::encode(&sha2::Sha256::digest(shared)[..4]);
        {
            let mut keys = self.e2ee_keys.lock().await;
            keys.insert((peer_id, session), shared);
        }
        {
            let mut ready = self.e2ee_ready.lock().await;
            let entry = ready
                .entry((peer_id, session))
                .or_insert(E2eeReadyState {
                    local_ready: false,
                    remote_ready: false,
                });
            entry.local_ready = true;
        }
        let response = ControlMessage::E2eeResp {
            session,
            from: self.identity.peer_id,
            public_key: keypair.public.to_bytes(),
            signature: sign_e2ee_public(&self.identity, &session.0, &keypair.public),
        };
        self.send_control_to_peer(peer_id, response, session).await?;
        tracing::info!(
            peer = %peer_id,
            session = ?session,
            key = %shared_fingerprint,
            "e2ee key derived (resp)"
        );
        let ready_msg = ControlMessage::EncryptedReady { session, alg: 1 };
        let _ = self.send_control_to_peer(peer_id, ready_msg, session).await;
        tracing::info!(peer = %peer_id, session = ?session, "e2ee response sent");
        Ok(())
    }

    async fn handle_e2ee_resp(
        &self,
        peer_id: PeerId,
        session: SessionId,
        public_key: [u8; 32],
        signature: Vec<u8>,
    ) -> Result<()> {
        {
            let keys = self.e2ee_keys.lock().await;
            if keys.contains_key(&(peer_id, session)) {
                return Ok(());
            }
        }
        let pending = {
            let mut pending = self.e2ee_pending.lock().await;
            pending.remove(&(peer_id, session))
        };
        let Some(pending) = pending else {
            return Ok(());
        };
        let peer_public = {
            let keys = self.peer_public_keys.lock().await;
            keys.get(&peer_id).cloned()
        };
        let Some(peer_public) = peer_public else {
            return Ok(());
        };
        let Some(peer_public) = ed25519_public_from_bytes(&peer_public) else {
            return Ok(());
        };
        let remote_public = public_key_from_bytes(public_key);
        if !verify_e2ee_public(&peer_public, &session.0, &remote_public, &signature) {
            tracing::warn!(peer = %peer_id, "e2ee response signature invalid");
            return Ok(());
        }
        let shared = derive_e2ee_shared_key(&pending.secret, &remote_public, &session.0);
        let shared_fingerprint = hex::encode(&sha2::Sha256::digest(shared)[..4]);
        {
            let mut keys = self.e2ee_keys.lock().await;
            keys.insert((peer_id, session), shared);
        }
        {
            let mut ready = self.e2ee_ready.lock().await;
            let entry = ready
                .entry((peer_id, session))
                .or_insert(E2eeReadyState {
                    local_ready: false,
                    remote_ready: false,
                });
            entry.local_ready = true;
        }
        let ready_msg = ControlMessage::EncryptedReady { session, alg: 1 };
        let _ = self.send_control_to_peer(peer_id, ready_msg, session).await;
        tracing::info!(
            peer = %peer_id,
            session = ?session,
            key = %shared_fingerprint,
            "e2ee key derived (init)"
        );
        tracing::info!(peer = %peer_id, session = ?session, "e2ee key established");
        Ok(())
    }

    async fn maybe_encrypt_payload(
        &self,
        payload: RiftPayload,
        header: &RiftFrameHeader,
        peer_id: Option<PeerId>,
        session: SessionId,
    ) -> Result<RiftPayload> {
        match payload {
            RiftPayload::Relay { target, inner } => {
                if should_encrypt(&inner) {
                    let use_group = self.should_use_group_key(session).await
                        && matches!(*inner, RiftPayload::Voice(_));
                let key = if use_group {
                    self.e2ee_key.ok_or_else(|| anyhow!("missing e2ee key"))?
                } else {
                    let keys = self.e2ee_keys.lock().await;
                    peer_id
                        .and_then(|peer| keys.get(&(peer, session)).copied())
                        .or(self.e2ee_key)
                        .ok_or_else(|| anyhow!("missing e2ee key"))?
                };
                if !use_group {
                    if let Some(peer) = peer_id {
                        let ready = self.e2ee_ready.lock().await;
                        let ready = ready
                            .get(&(peer, session))
                            .map(|state| state.local_ready && state.remote_ready)
                            .unwrap_or(false);
                        if !ready {
                            tracing::debug!(
                                target: "security",
                                peer = %peer,
                                session = ?session,
                                "e2ee not ready; sending plaintext"
                            );
                            return Ok(RiftPayload::Relay { target, inner });
                        }
                    }
                }
                let encrypted = RiftPayload::Encrypted(
                    encrypt_payload_with_key(&key, header, &inner)?
                );
                Ok(RiftPayload::Relay {
                    target,
                    inner: Box::new(encrypted),
                    })
                } else {
                    Ok(RiftPayload::Relay { target, inner })
                }
            }
            other => {
                if !should_encrypt(&other) {
                    return Ok(other);
                }
                let use_group = self.should_use_group_key(session).await
                    && matches!(other, RiftPayload::Voice(_));
                let key = if use_group {
                    self.e2ee_key.ok_or_else(|| anyhow!("missing e2ee key"))?
                } else {
                    let keys = self.e2ee_keys.lock().await;
                    peer_id
                        .and_then(|peer| keys.get(&(peer, session)).copied())
                        .or(self.e2ee_key)
                        .ok_or_else(|| anyhow!("missing e2ee key"))?
                };
                if !use_group {
                    if let Some(peer) = peer_id {
                        let ready = self.e2ee_ready.lock().await;
                        let ready = ready
                            .get(&(peer, session))
                            .map(|state| state.local_ready && state.remote_ready)
                            .unwrap_or(false);
                        if !ready {
                            tracing::debug!(
                                target: "security",
                                peer = %peer,
                                session = ?session,
                                "e2ee not ready; sending plaintext"
                            );
                            return Ok(other);
                        }
                    }
                }
                let encrypted = encrypt_payload_with_key(&key, header, &other)?;
                Ok(RiftPayload::Encrypted(encrypted))
            }
        }
    }

    async fn decrypt_payload(
        &self,
        header: &RiftFrameHeader,
        encrypted: EncryptedPayload,
        peer_id: PeerId,
        session: SessionId,
    ) -> Result<RiftPayload> {
        let key = {
            let keys = self.e2ee_keys.lock().await;
            keys.get(&(peer_id, session)).copied()
        }
        .or(self.e2ee_key)
        .ok_or_else(|| anyhow!("missing e2ee key"))?;
        decrypt_payload_with_key(&key, header, encrypted)
    }

    async fn update_link_stats(&self, peer_id: PeerId, seq: u32, sent_ms: u64) {
        if peer_id == self.identity.peer_id {
            return;
        }
        let arrival_ms = now_timestamp();
        let mut stats_map = self.link_stats.lock().await;
        let state = stats_map.entry(peer_id).or_insert_with(LinkStatsState::new);
        state.update_on_receive(seq, sent_ms, arrival_ms);
        let should_emit = state.last_emit.elapsed() >= Duration::from_millis(500);
        let stats = state.snapshot();
        if should_emit {
            state.last_emit = tokio::time::Instant::now();
        }
        drop(stats_map);
        if should_emit {
            let global = self.global_stats_snapshot().await;
            let _ = self
                .events_tx
                .send(MeshEvent::StatsUpdate {
                    peer: peer_id,
                    stats,
                    global,
                })
                .await;
            metrics::observe_histogram(
                "rift_rtt_ms",
                &[("peer", &peer_id.to_hex())],
                stats.rtt_ms as f64,
            );
            metrics::set_gauge(
                "rift_packet_loss",
                &[("peer", &peer_id.to_hex())],
                stats.loss as f64,
            );
            metrics::set_gauge(
                "rift_jitter_ms",
                &[("peer", &peer_id.to_hex())],
                stats.jitter_ms as f64,
            );
            self.consider_route(peer_id, stats).await;
        }
    }

    async fn update_rtt(&self, peer_id: PeerId, sent_at_ms: u64) {
        let now_ms = now_timestamp();
        let rtt_ms = now_ms.saturating_sub(sent_at_ms) as f32;
        let mut stats_map = self.link_stats.lock().await;
        let state = stats_map.entry(peer_id).or_insert_with(LinkStatsState::new);
        state.update_rtt(rtt_ms);
        let stats = state.snapshot();
        drop(stats_map);
        let global = self.global_stats_snapshot().await;
        let _ = self
            .events_tx
            .send(MeshEvent::StatsUpdate {
                peer: peer_id,
                stats,
                global,
            })
            .await;
        metrics::observe_histogram(
            "rift_rtt_ms",
            &[("peer", &peer_id.to_hex())],
            stats.rtt_ms as f64,
        );
        self.consider_route(peer_id, stats).await;
    }

    async fn allow_packet(&self, addr: SocketAddr) -> bool {
        let now = Instant::now();
        let mut limits = self.rate_limits.lock().await;
        let entry = limits.entry(addr).or_insert_with(|| RateLimitState::new(now));
        let allowed = entry.allow(now);
        if !allowed {
            if now.duration_since(entry.last_drop) >= RATE_LIMIT_WINDOW {
                entry.last_drop = now;
                tracing::warn!(target = "security", %addr, "rate limit exceeded");
            }
            metrics::inc_counter("rift_packets_dropped", &[("reason", "rate_limit")]);
        }
        allowed
    }

    #[cfg(feature = "predictive-rendezvous")]
    async fn try_handle_pr_probe(&self, addr: SocketAddr, data: &[u8]) -> Result<bool> {
        let parsed = match parse_probe_payload(data) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(false),
        };

        let rendezvous_id = parsed.rendezvous_id;
        let tx = {
            let sessions = self.pr_sessions.lock().await;
            sessions.get(&rendezvous_id).map(|session| session.tx.clone())
        };

        let Some(tx) = tx else {
            return Ok(false);
        };

        if tx.send((addr, parsed)).await.is_err() {
            let mut sessions = self.pr_sessions.lock().await;
            sessions.remove(&rendezvous_id);
        }

        Ok(true)
    }

    #[cfg(feature = "predictive-rendezvous")]
    async fn register_pr_session(
        &self,
        rendezvous_id: u64,
        tx: mpsc::Sender<(SocketAddr, ParsedProbe)>,
    ) -> Result<()> {
        let mut sessions = self.pr_sessions.lock().await;
        if sessions.contains_key(&rendezvous_id) {
            return Err(anyhow!("predictive rendezvous session already exists"));
        }
        sessions.insert(rendezvous_id, PrSession { tx });
        Ok(())
    }

    #[cfg(feature = "predictive-rendezvous")]
    async fn unregister_pr_session(&self, rendezvous_id: u64) {
        let mut sessions = self.pr_sessions.lock().await;
        sessions.remove(&rendezvous_id);
    }

    #[cfg(feature = "predictive-rendezvous")]
    async fn primary_local_port(&self) -> Result<u16> {
        let sockets = self.sockets.lock().await;
        match sockets.first() {
            Some(MeshSocket::Udp(sock)) => Ok(sock.local_addr()?.port()),
            Some(MeshSocket::Turn(relay)) => Ok(relay.relay_addr().port()),
            None => Err(anyhow!("missing socket")),
        }
    }

    async fn record_send(&self, addr: SocketAddr, bytes: usize) {
        let peer_id = {
            let connections = self.connections.lock().await;
            connections.get(&addr).and_then(|conn| conn.peer_id)
        };
        let mut global = self.global_traffic.lock().await;
        global.packets_sent = global.packets_sent.saturating_add(1);
        global.bytes_sent = global.bytes_sent.saturating_add(bytes as u64);
        drop(global);
        metrics::inc_counter("rift_packets_sent", &[]);
        metrics::add_counter("rift_bytes_sent", &[], bytes as u64);
        if let Some(peer_id) = peer_id {
            let mut peer = self.peer_traffic.lock().await;
            let entry = peer.entry(peer_id).or_default();
            entry.packets_sent = entry.packets_sent.saturating_add(1);
            entry.bytes_sent = entry.bytes_sent.saturating_add(bytes as u64);
            drop(peer);
            metrics::inc_counter("rift_packets_sent", &[("peer", &peer_id.to_hex())]);
            metrics::add_counter("rift_bytes_sent", &[("peer", &peer_id.to_hex())], bytes as u64);
        }
    }

    async fn record_recv(&self, addr: SocketAddr, bytes: usize) {
        let peer_id = {
            let connections = self.connections.lock().await;
            connections.get(&addr).and_then(|conn| conn.peer_id)
        };
        let mut global = self.global_traffic.lock().await;
        global.packets_received = global.packets_received.saturating_add(1);
        global.bytes_received = global.bytes_received.saturating_add(bytes as u64);
        drop(global);
        metrics::inc_counter("rift_packets_received", &[]);
        metrics::add_counter("rift_bytes_received", &[], bytes as u64);
        if let Some(peer_id) = peer_id {
            let mut peer = self.peer_traffic.lock().await;
            let entry = peer.entry(peer_id).or_default();
            entry.packets_received = entry.packets_received.saturating_add(1);
            entry.bytes_received = entry.bytes_received.saturating_add(bytes as u64);
            drop(peer);
            metrics::inc_counter("rift_packets_received", &[("peer", &peer_id.to_hex())]);
            metrics::add_counter(
                "rift_bytes_received",
                &[("peer", &peer_id.to_hex())],
                bytes as u64,
            );
        }
    }

    async fn global_stats_snapshot(&self) -> GlobalStats {
        let global = self.global_traffic.lock().await.clone();
        let peers = self.peers_by_id.lock().await.len() + 1;
        let sessions = self.session_mgr.lock().await.sessions.len();
        GlobalStats {
            num_peers: peers,
            num_sessions: sessions,
            packets_sent: global.packets_sent,
            packets_received: global.packets_received,
            bytes_sent: global.bytes_sent,
            bytes_received: global.bytes_received,
        }
    }

    async fn emit_global_metrics(&self) {
        let global = self.global_stats_snapshot().await;
        metrics::set_gauge("rift_number_of_peers", &[], global.num_peers as f64);
        metrics::set_gauge("rift_number_of_sessions", &[], global.num_sessions as f64);
    }

    async fn consider_route(&self, peer_id: PeerId, stats: LinkStats) {
        let qos = &self.qos;
        let prefer_relay =
            stats.loss > qos.packet_loss_tolerance || stats.rtt_ms > qos.max_latency_ms as f32;
        if prefer_relay {
            let relay = {
                let relays = self.relay_candidates.lock().await;
                relays.get(&peer_id).copied()
            };
            if let Some(relay) = relay {
                let relay_ok = {
                    let peers = self.peers_by_id.lock().await;
                    peers.contains_key(&relay)
                };
                if relay_ok {
                    let mut routes = self.routes.lock().await;
                    if !matches!(routes.get(&peer_id), Some(PeerRoute::Relayed { .. })) {
                        routes.insert(peer_id, PeerRoute::Relayed { via: relay });
                        drop(routes);
                        let _ = self
                            .events_tx
                            .send(MeshEvent::RouteUpdated {
                                peer_id,
                                route: PeerRoute::Relayed { via: relay },
                            })
                            .await;
                        tracing::info!(peer = %peer_id, relay = %relay, "switching to relay route");
                    }
                }
            }
            return;
        }

        let direct_addr = {
            let peers = self.peers_by_id.lock().await;
            peers.get(&peer_id).copied()
        };
        if let Some(addr) = direct_addr {
            let mut routes = self.routes.lock().await;
            if !matches!(routes.get(&peer_id), Some(PeerRoute::Direct { .. })) {
                routes.insert(peer_id, PeerRoute::Direct { addr });
                drop(routes);
                let _ = self
                    .events_tx
                    .send(MeshEvent::RouteUpdated {
                        peer_id,
                        route: PeerRoute::Direct { addr },
                    })
                    .await;
                tracing::info!(peer = %peer_id, "switching to direct route");
            }
        }
    }

    async fn send_payload(
        &self,
        addr: SocketAddr,
        payload: RiftPayload,
        seq: u32,
        timestamp: u64,
        session: SessionId,
        peer_id: Option<PeerId>,
    ) -> Result<()> {
        self.send_payload_with_source(
            addr,
            payload,
            seq,
            timestamp,
            self.identity.peer_id,
            session,
            peer_id,
        )
        .await
    }

    async fn send_payload_with_source(
        &self,
        addr: SocketAddr,
        payload: RiftPayload,
        seq: u32,
        timestamp: u64,
        source: PeerId,
        session: SessionId,
        peer_id: Option<PeerId>,
    ) -> Result<()> {
        let stream = stream_for_payload(&payload);
        let header = RiftFrameHeader {
            version: ProtocolVersion::V1,
            stream,
            flags: 0,
            seq,
            timestamp,
            source,
            session,
        };
        let payload = self
            .maybe_encrypt_payload(payload, &header, peer_id, session)
            .await?;
        let plaintext = encode_frame(&header, &payload);
        let (ciphertext, socket_idx) = {
            let mut connections = self.connections.lock().await;
            let Some(conn) = connections.get_mut(&addr) else {
                return Err(anyhow!("missing connection"));
            };
            let mut out = vec![0u8; plaintext.len() + 128];
            let len = conn.session.encrypt(&plaintext, &mut out)?;
            out.truncate(len);
            (out, conn.socket_idx)
        };
        self.send_raw(socket_idx, addr, &ciphertext).await?;
        self.record_send(addr, ciphertext.len()).await;
        Ok(())
    }

    async fn send_frame(
        &self,
        addr: SocketAddr,
        header: &RiftFrameHeader,
        payload: &RiftPayload,
    ) -> Result<()> {
        let plaintext = encode_frame(header, payload);
        let (ciphertext, socket_idx) = {
            let mut connections = self.connections.lock().await;
            let Some(conn) = connections.get_mut(&addr) else {
                return Err(anyhow!("missing connection"));
            };
            let mut out = vec![0u8; plaintext.len() + 128];
            let len = conn.session.encrypt(&plaintext, &mut out)?;
            out.truncate(len);
            (out, conn.socket_idx)
        };
        self.send_raw(socket_idx, addr, &ciphertext).await?;
        self.record_send(addr, ciphertext.len()).await;
        Ok(())
    }

    async fn forward_group_voice(
        &self,
        header: &RiftFrameHeader,
        payload: &RiftPayload,
    ) -> Result<()> {
        let session = header.session;
        let from = header.source;
        let participants = {
            let mgr = self.session_mgr.lock().await;
            mgr.participants(session)
        };
        let routes = self.routes_snapshot().await;
        for peer_id in participants {
            if peer_id == self.identity.peer_id || peer_id == from {
                continue;
            }
            let Some(route) = routes.get(&peer_id).cloned() else {
                continue;
            };
            match route {
                PeerRoute::Direct { addr } => {
                    let _ = self.send_frame(addr, header, payload).await;
                }
                PeerRoute::Relayed { via } => {
                    let relay_addr = {
                        let peers = self.peers_by_id.lock().await;
                        peers.get(&via).copied()
                    };
                    let Some(relay_addr) = relay_addr else {
                        continue;
                    };
                    let envelope = RiftPayload::Relay {
                        target: peer_id,
                        inner: Box::new(payload.clone()),
                    };
                    let _ = self.send_frame(relay_addr, header, &envelope).await;
                }
            }
        }
        Ok(())
    }

    async fn handle_frame(
        self: Arc<Self>,
        addr: SocketAddr,
        header: RiftFrameHeader,
        payload: RiftPayload,
    ) -> Result<()> {
        if self.auth_required
            && !matches!(
                payload,
                RiftPayload::Control(ControlMessage::Auth { .. })
                    | RiftPayload::Control(ControlMessage::Join { .. })
                    | RiftPayload::Control(ControlMessage::Hello { .. })
                    | RiftPayload::Control(ControlMessage::IceCandidates { .. })
                    | RiftPayload::Control(ControlMessage::IceCheck { .. })
                    | RiftPayload::Control(ControlMessage::IceCheckAck { .. })
                    | RiftPayload::Control(ControlMessage::E2eeInit { .. })
                    | RiftPayload::Control(ControlMessage::E2eeResp { .. })
                    | RiftPayload::Control(ControlMessage::EncryptedReady { .. })
                    | RiftPayload::Control(ControlMessage::PeerState { .. })
            )
        {
            let authenticated = {
                let connections = self.connections.lock().await;
                connections
                    .get(&addr)
                    .map(|conn| conn.authenticated)
                    .unwrap_or(false)
            };
            if !authenticated {
                tracing::warn!(%addr, "unauthenticated peer message rejected");
                return Ok(());
            }
        }
        match payload {
            RiftPayload::Control(ControlMessage::Join { peer_id, .. }) => {
                let mut connections = self.connections.lock().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    conn.peer_id = Some(peer_id);
                }
                drop(connections);

                let mut peers = self.peers_by_id.lock().await;
                peers.insert(peer_id, addr);
                drop(peers);

                let mut peer_addrs = self.peer_addrs.lock().await;
                peer_addrs.insert(peer_id, addr);
                drop(peer_addrs);
                let mut peer_candidates = self.peer_candidates.lock().await;
                peer_candidates
                    .entry(peer_id)
                    .or_insert_with(|| vec![addr]);
                drop(peer_candidates);

                let mut routes = self.routes.lock().await;
                let upgraded = matches!(routes.get(&peer_id), Some(PeerRoute::Relayed { .. }));
                routes.insert(peer_id, PeerRoute::Direct { addr });
                drop(routes);

                let mut sessions = self.session_mgr.lock().await;
                sessions.add_participant(self.channel_session, peer_id);
                drop(sessions);
                let _ = self.update_group_topology(self.channel_session).await;

                let mut caps = self.peer_capabilities.lock().await;
                caps.entry(peer_id).or_insert_with(default_peer_capabilities);
                drop(caps);
                self.update_group_codec().await;

                let _ = self.events_tx.send(MeshEvent::PeerJoined(peer_id)).await;
                let _ = self
                    .events_tx
                    .send(MeshEvent::RouteUpdated {
                        peer_id,
                        route: PeerRoute::Direct { addr },
                    })
                    .await;
                self.emit_global_metrics().await;
                if upgraded {
                    let _ = self.events_tx.send(MeshEvent::RouteUpgraded(peer_id)).await;
                    tracing::info!(peer = %peer_id, "route upgraded to direct");
                }

                self.send_peer_list(addr).await?;
                // Broadcast updated peer list to all existing peers so they know about the new peer
                let all_peers: Vec<SocketAddr> = {
                    let peers_by_id = self.peers_by_id.lock().await;
                    peers_by_id.values().copied().collect()
                };
                for peer_addr in all_peers {
                    if peer_addr != addr {
                        let _ = self.send_peer_list(peer_addr).await;
                    }
                }
            }
            RiftPayload::Control(ControlMessage::PeerState {
                peer_id,
                relay_capable,
            }) => {
                let mut peer_caps = self.peer_caps.lock().await;
                peer_caps.insert(peer_id, relay_capable);
                drop(peer_caps);
            }
            RiftPayload::Control(ControlMessage::Hello { peer_id, public_key, capabilities, candidates }) => {
                {
                    let mut keys = self.peer_public_keys.lock().await;
                    keys.insert(peer_id, public_key.clone());
                }
                if !candidates.is_empty() {
                    let mut peer_candidates = self.peer_candidates.lock().await;
                    peer_candidates.insert(peer_id, candidates);
                }
                let _ = self
                    .events_tx
                    .send(MeshEvent::PeerIdentity {
                        peer_id,
                        public_key: public_key.clone(),
                    })
                    .await;
                self.handle_capabilities(peer_id, capabilities).await?;
                let authenticated = {
                    let connections = self.connections.lock().await;
                    connections
                        .get(&addr)
                        .map(|conn| conn.authenticated)
                        .unwrap_or(false)
                };
                if !self.auth_required || authenticated {
                    let _ = self.start_e2ee(peer_id, self.channel_session).await;
                }
                let _ = self.send_ice_candidates(peer_id, SessionId::NONE).await;
            }
            RiftPayload::Control(ControlMessage::IceCandidates { peer_id, candidates, .. }) => {
                if !candidates.is_empty() {
                    let mut peer_ice = self.peer_ice_candidates.lock().await;
                    peer_ice.insert(peer_id, candidates.clone());
                    drop(peer_ice);
                    let mut peer_candidates = self.peer_candidates.lock().await;
                    let addrs: Vec<SocketAddr> = candidates.into_iter().map(|cand| cand.addr).collect();
                    peer_candidates.insert(peer_id, addrs);
                }
            }
            RiftPayload::Control(ControlMessage::IceCheck { session, candidate, .. }) => {
                let ack = ControlMessage::IceCheckAck { session, candidate };
                let from = header.source;
                let _ = self.send_control_to_peer(from, ack, session).await;
            }
            RiftPayload::Control(ControlMessage::IceCheckAck { candidate, .. }) => {
                let peer_id = header.source;
                let addr = candidate.addr;
                let mut peer_addrs = self.peer_addrs.lock().await;
                peer_addrs.insert(peer_id, addr);
                drop(peer_addrs);

                let mut routes = self.routes.lock().await;
                let upgraded = matches!(routes.get(&peer_id), Some(PeerRoute::Relayed { .. }));
                if !matches!(routes.get(&peer_id), Some(PeerRoute::Direct { .. })) {
                    routes.insert(peer_id, PeerRoute::Direct { addr });
                }
                drop(routes);

                let _ = self
                    .events_tx
                    .send(MeshEvent::RouteUpdated {
                        peer_id,
                        route: PeerRoute::Direct { addr },
                    })
                    .await;
                if upgraded {
                    let _ = self.events_tx.send(MeshEvent::RouteUpgraded(peer_id)).await;
                }
            }
            RiftPayload::Control(ControlMessage::KeyInit { .. })
            | RiftPayload::Control(ControlMessage::KeyResp { .. }) => {
                tracing::debug!(target: "security", "legacy e2ee control message ignored");
            }
            RiftPayload::Control(ControlMessage::CapabilitiesUpdate(capabilities)) => {
                let peer_id = header.source;
                self.handle_capabilities(peer_id, capabilities).await?;
            }
            RiftPayload::Control(ControlMessage::Leave { peer_id }) => {
                let mut peers = self.peers_by_id.lock().await;
                peers.remove(&peer_id);
                drop(peers);
                let mut peer_caps = self.peer_caps.lock().await;
                peer_caps.remove(&peer_id);
                drop(peer_caps);
                let mut peer_keys = self.peer_public_keys.lock().await;
                peer_keys.remove(&peer_id);
                drop(peer_keys);
                let mut routes = self.routes.lock().await;
                routes.remove(&peer_id);
                let removed: Vec<PeerId> = routes
                    .iter()
                    .filter_map(|(pid, route)| match route {
                        PeerRoute::Relayed { via } if *via == peer_id => Some(*pid),
                        _ => None,
                    })
                    .collect();
                for pid in removed {
                    routes.remove(&pid);
                }
                drop(routes);
                let mut peer_addrs = self.peer_addrs.lock().await;
                peer_addrs.remove(&peer_id);
                drop(peer_addrs);
                let mut peer_candidates = self.peer_candidates.lock().await;
                peer_candidates.remove(&peer_id);
                drop(peer_candidates);
                let mut peer_ice = self.peer_ice_candidates.lock().await;
                peer_ice.remove(&peer_id);
                drop(peer_ice);
                let mut relay_candidates = self.relay_candidates.lock().await;
                relay_candidates.remove(&peer_id);
                drop(relay_candidates);
                let mut e2ee_keys = self.e2ee_keys.lock().await;
                e2ee_keys.retain(|(peer, _), _| *peer != peer_id);
                drop(e2ee_keys);
                let mut e2ee_pending = self.e2ee_pending.lock().await;
                e2ee_pending.retain(|(peer, _), _| *peer != peer_id);
                drop(e2ee_pending);
                let mut e2ee_ready = self.e2ee_ready.lock().await;
                e2ee_ready.retain(|(peer, _), _| *peer != peer_id);
                drop(e2ee_ready);
                let mut stats = self.link_stats.lock().await;
                stats.remove(&peer_id);
                drop(stats);
                let mut sessions = self.session_mgr.lock().await;
                sessions.remove_participant_all(peer_id);
                drop(sessions);
                let _ = self.update_group_topology(self.channel_session).await;
                let active = *self.active_session.lock().await;
                if active != SessionId::NONE && active != self.channel_session {
                    let _ = self.update_group_topology(active).await;
                }
                let _ = self.events_tx.send(MeshEvent::PeerLeft(peer_id)).await;
                self.emit_global_metrics().await;
            }
            RiftPayload::Control(ControlMessage::Chat(chat)) => {
                let mut cache = self.cache.lock().await;
                if cache.contains(&chat.id) {
                    return Ok(());
                }
                cache.insert(chat.id);
                drop(cache);

                let _ = self
                    .events_tx
                    .send(MeshEvent::ChatReceived(chat.clone()))
                    .await;
            }
            RiftPayload::Control(ControlMessage::Ping { nonce, sent_at_ms }) => {
                let from = header.source;
                let pong = ControlMessage::Pong { nonce, sent_at_ms };
                let _ = self.send_control_to_peer(from, pong, SessionId::NONE).await;
            }
            RiftPayload::Control(ControlMessage::Pong { sent_at_ms, .. }) => {
                let from = header.source;
                self.update_rtt(from, sent_at_ms).await;
            }
            RiftPayload::Control(ControlMessage::Auth { token }) => {
                let expected = self.auth_token.clone().unwrap_or_default();
                if self.auth_required && token != expected {
                    tracing::warn!(%addr, "auth token mismatch");
                    self.disconnect_addr(addr).await;
                    return Ok(());
                }
                let mut connections = self.connections.lock().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    conn.authenticated = true;
                }
            }
            RiftPayload::Control(ControlMessage::PeerList { peers }) => {
                self.handle_peer_list(addr, peers).await?;
            }
            RiftPayload::Control(ControlMessage::Group(group)) => {
                match group {
                    GroupControl::Join { session, peer_id, .. } => {
                        let mut sessions = self.session_mgr.lock().await;
                        sessions.add_participant(session, peer_id);
                        drop(sessions);
                        let _ = self.update_group_topology(session).await;
                    }
                    GroupControl::Leave { session, peer_id } => {
                        let mut sessions = self.session_mgr.lock().await;
                        sessions.remove_participant(session, peer_id);
                        drop(sessions);
                        let _ = self.update_group_topology(session).await;
                    }
                    GroupControl::Topology { session, mode } => {
                        self.set_group_mode(session, mode).await;
                        let _ = self
                            .events_tx
                            .send(MeshEvent::GroupTopology { session, mode })
                            .await;
                    }
                    GroupControl::StreamPublish { .. } | GroupControl::StreamSubscribe { .. } => {}
                }
            }
            RiftPayload::Control(ControlMessage::Call(call)) => {
                self.handle_call(call).await?;
            }
            RiftPayload::Control(ControlMessage::E2eeInit {
                session,
                from,
                public_key,
                signature,
            }) => {
                self.handle_e2ee_init(from, session, public_key, signature).await?;
            }
            RiftPayload::Control(ControlMessage::E2eeResp {
                session,
                from,
                public_key,
                signature,
            }) => {
                self.handle_e2ee_resp(from, session, public_key, signature).await?;
            }
            RiftPayload::Control(ControlMessage::EncryptedReady { session, .. }) => {
                let peer_id = header.source;
                let mut ready = self.e2ee_ready.lock().await;
                let entry = ready
                    .entry((peer_id, session))
                    .or_insert(E2eeReadyState {
                        local_ready: false,
                        remote_ready: false,
                    });
                entry.remote_ready = true;
            }
            RiftPayload::Voice(VoicePacket { codec_id, payload }) => {
                let from = header.source;
                let seq = header.seq;
                let timestamp = header.timestamp;
                let session = header.session;
                let codec = codec_id;
                let active_session = *self.active_session.lock().await;
                if active_session != SessionId::NONE && session != active_session {
                    return Ok(());
                }
                let mut seqs = self.voice_seq.lock().await;
                if let Some(last) = seqs.get(&from) {
                    if seq <= *last {
                        return Ok(());
                    }
                }
                seqs.insert(from, seq);
                drop(seqs);
                if self.should_forward_voice(session, from).await {
                    let payload = RiftPayload::Voice(VoicePacket {
                        codec_id: codec,
                        payload: payload.clone(),
                    });
                    let _ = self.forward_group_voice(&header, &payload).await;
                }

                let _ = self
                    .events_tx
                    .send(MeshEvent::VoiceFrame {
                        from,
                        seq,
                        timestamp,
                        session,
                        codec,
                        payload: payload.clone(),
                    })
                    .await;
            }
            RiftPayload::Encrypted(encrypted) => {
                let from = header.source;
                let session = header.session;
                if header.stream == StreamKind::Voice
                    && self.should_forward_voice(session, from).await
                {
                    let payload = RiftPayload::Encrypted(encrypted.clone());
                    let _ = self.forward_group_voice(&header, &payload).await;
                }
                if let Ok(inner) = self.decrypt_payload(&header, encrypted, from, session).await {
                    Box::pin(self.handle_frame(addr, header, inner)).await?;
                } else {
                    let has_key = {
                        let keys = self.e2ee_keys.lock().await;
                        keys.contains_key(&(from, session))
                    };
                    let key_fingerprint = {
                        let keys = self.e2ee_keys.lock().await;
                        keys.get(&(from, session))
                            .map(|key| hex::encode(&sha2::Sha256::digest(key)[..4]))
                    };
                    let ready = {
                        let ready = self.e2ee_ready.lock().await;
                        ready
                            .get(&(from, session))
                            .map(|state| state.local_ready && state.remote_ready)
                            .unwrap_or(false)
                    };
                    tracing::warn!(
                        %addr,
                        peer = %from,
                        session = ?session,
                        has_key,
                        key = key_fingerprint.as_deref().unwrap_or("none"),
                        ready,
                        "e2ee decrypt failed"
                    );
                }
            }
            RiftPayload::Relay { target, inner } => {
                if target == self.identity.peer_id {
                    Box::pin(self.handle_frame(addr, header, *inner)).await?;
                } else {
                    self.forward_relay(target, header, *inner).await?;
                }
            }
            RiftPayload::Text(chat) => {
                let mut cache = self.cache.lock().await;
                if cache.contains(&chat.id) {
                    return Ok(());
                }
                cache.insert(chat.id);
                drop(cache);
                let _ = self
                    .events_tx
                    .send(MeshEvent::ChatReceived(chat.clone()))
                    .await;
            }
            RiftPayload::Control(ControlMessage::RouteInfo { .. })
            | RiftPayload::Control(ControlMessage::Capabilities(_)) => {}
        }
        Ok(())
    }

    async fn handle_peer_list(self: Arc<Self>, addr: SocketAddr, peers: Vec<PeerInfo>) -> Result<()> {
        // Always register peers as session participants for topology tracking
        for peer in &peers {
            if peer.peer_id == self.identity.peer_id {
                continue;
            }
            let mut sessions = self.session_mgr.lock().await;
            sessions.add_participant(self.channel_session, peer.peer_id);
        }
        // Trigger topology update after learning about new participants
        let _ = self.update_group_topology(self.channel_session).await;

        let nat_cfg = { self.nat_cfg.lock().await.clone() };
        let Some(nat_cfg) = nat_cfg else {
            return Ok(());
        };

        let relay_peer_id = {
            let connections = self.connections.lock().await;
            connections.get(&addr).and_then(|conn| conn.peer_id)
        };
        let relay_capable = if let Some(peer_id) = relay_peer_id {
            let peer_caps = self.peer_caps.lock().await;
            peer_caps.get(&peer_id).copied().unwrap_or(false)
        } else {
            false
        };

        for peer in peers {
            if peer.peer_id == self.identity.peer_id {
                continue;
            }
            let already = {
                let peers_by_id = self.peers_by_id.lock().await;
                peers_by_id.contains_key(&peer.peer_id)
            };
            let mut peer_caps = self.peer_caps.lock().await;
            peer_caps.insert(peer.peer_id, peer.relay_capable);
            drop(peer_caps);
            let mut addrs = if peer.addrs.is_empty() {
                vec![peer.addr]
            } else {
                peer.addrs.clone()
            };
            addrs.sort();
            addrs.dedup();
            let primary_addr = addrs[0];
            let mut peer_addrs = self.peer_addrs.lock().await;
            peer_addrs.insert(peer.peer_id, primary_addr);
            drop(peer_addrs);
            let mut peer_candidates = self.peer_candidates.lock().await;
            peer_candidates.insert(peer.peer_id, addrs.clone());
            drop(peer_candidates);

            if already {
                continue;
            }
            let already = {
                let connections = self.connections.lock().await;
                connections.contains_key(&primary_addr)
            };
            if already {
                continue;
            }
            let endpoint = PeerEndpoint {
                peer_id: PeerId([0u8; 32]),
                external_addrs: addrs.clone(),
                punch_ports: addrs.iter().map(|addr| addr.port()).collect(),
            };
            if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                if let Ok(socket_idx) = self.add_socket(socket).await {
                    if let Err(err) = self.initiate_handshake(remote, socket_idx).await {
                        tracing::warn!("handshake to {} failed: {err}", remote);
                    }
                }
            } else if relay_capable {
                if let Some(relay_peer_id) = relay_peer_id {
                    let mut relay_candidates = self.relay_candidates.lock().await;
                    relay_candidates.insert(peer.peer_id, relay_peer_id);
                    drop(relay_candidates);
                    let mut routes = self.routes.lock().await;
                    if !matches!(routes.get(&peer.peer_id), Some(PeerRoute::Direct { .. })) {
                        routes.insert(peer.peer_id, PeerRoute::Relayed { via: relay_peer_id });
                        drop(routes);
                        let _ = self
                            .events_tx
                            .send(MeshEvent::RouteUpdated {
                                peer_id: peer.peer_id,
                                route: PeerRoute::Relayed { via: relay_peer_id },
                            })
                            .await;
                        tracing::info!(
                            peer = %peer.peer_id,
                            relay = %relay_peer_id,
                            "relay route established"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn start_call(&self, to: PeerId, rndzv_srt_uri: Option<String>) -> Result<SessionId> {
        let session = SessionId::random();
        tracing::info!(to = %to, session = ?session, "call start");
        {
            let mut sessions = self.session_mgr.lock().await;
            sessions.set_state(session, CallState::Ringing);
            sessions.add_participant(session, self.identity.peer_id);
            sessions.add_participant(session, to);
        }
        self.emit_global_metrics().await;
        let call = CallControl::Invite {
            session,
            from: self.identity.peer_id,
            to,
            display_name: None,
            rndzv_srt_uri,
        };
        self.send_call_to_peer(to, call, session).await?;
        Ok(session)
    }

    async fn accept_call(&self, session: SessionId) -> Result<()> {
        tracing::info!(session = ?session, "call accept");
        {
            let mut sessions = self.session_mgr.lock().await;
            sessions.set_state(session, CallState::Active);
            sessions.add_participant(session, self.identity.peer_id);
        }
        self.emit_global_metrics().await;
        let participants = {
            let sessions = self.session_mgr.lock().await;
            sessions.participants(session)
        };
        for peer_id in participants {
            if peer_id == self.identity.peer_id {
                continue;
            }
            let _ = self.start_e2ee(peer_id, session).await;
            let call = CallControl::Accept {
                session,
                from: self.identity.peer_id,
            };
            let _ = self.send_call_to_peer(peer_id, call, session).await;
        }
        let mut active = self.active_session.lock().await;
        *active = session;
        Ok(())
    }

    async fn decline_call(&self, session: SessionId, reason: Option<String>) -> Result<()> {
        tracing::info!(session = ?session, ?reason, "call decline");
        {
            let mut sessions = self.session_mgr.lock().await;
            sessions.set_state(session, CallState::Ended);
        }
        self.emit_global_metrics().await;
        let participants = {
            let sessions = self.session_mgr.lock().await;
            sessions.participants(session)
        };
        for peer_id in participants {
            if peer_id == self.identity.peer_id {
                continue;
            }
            let call = CallControl::Decline {
                session,
                from: self.identity.peer_id,
                reason: reason.clone(),
            };
            let _ = self.send_call_to_peer(peer_id, call, session).await;
        }
        let mut active = self.active_session.lock().await;
        if *active == session {
            *active = self.channel_session;
        }
        Ok(())
    }

    async fn end_call(&self, session: SessionId) -> Result<()> {
        tracing::info!(session = ?session, "call end");
        {
            let mut sessions = self.session_mgr.lock().await;
            sessions.set_state(session, CallState::Ended);
        }
        self.emit_global_metrics().await;
        let participants = {
            let sessions = self.session_mgr.lock().await;
            sessions.participants(session)
        };
        for peer_id in participants {
            if peer_id == self.identity.peer_id {
                continue;
            }
            let call = CallControl::Bye {
                session,
                from: self.identity.peer_id,
            };
            let _ = self.send_call_to_peer(peer_id, call, session).await;
        }
        let mut active = self.active_session.lock().await;
        if *active == session {
            *active = self.channel_session;
        }
        Ok(())
    }

    async fn send_call_to_peer(
        &self,
        peer_id: PeerId,
        call: CallControl,
        session: SessionId,
    ) -> Result<()> {
        let payload = RiftPayload::Control(ControlMessage::Call(call));
        let routes = self.routes_snapshot().await;
        let Some(route) = routes.get(&peer_id).cloned() else {
            return Err(anyhow!("missing route"));
        };
        let seq = self.next_control_seq().await;
        self.send_to_peer(peer_id, route, payload, seq, now_timestamp(), session)
            .await
    }

    async fn handle_call(&self, call: CallControl) -> Result<()> {
        match call {
            CallControl::Invite {
                session,
                from,
                to,
                rndzv_srt_uri,
                ..
            } => {
                if to != self.identity.peer_id {
                    return Ok(());
                }
                {
                    let mut sessions = self.session_mgr.lock().await;
                    sessions.set_state(session, CallState::Ringing);
                    sessions.add_participant(session, from);
                    sessions.add_participant(session, to);
                }
                let _ = self.update_group_topology(session).await;
                let _ = self
                    .events_tx
                    .send(MeshEvent::IncomingCall {
                        session,
                        from,
                        rndzv_srt_uri,
                    })
                    .await;
            }
            CallControl::Accept { session, from } => {
                {
                    let mut sessions = self.session_mgr.lock().await;
                    sessions.set_state(session, CallState::Active);
                    sessions.add_participant(session, from);
                    sessions.add_participant(session, self.identity.peer_id);
                }
                let _ = self.update_group_topology(session).await;
                let mut active = self.active_session.lock().await;
                *active = session;
                let _ = self.start_e2ee(from, session).await;
                let _ = self
                    .events_tx
                    .send(MeshEvent::CallAccepted { session, from })
                    .await;
            }
            CallControl::Decline { session, from, reason } => {
                {
                    let mut sessions = self.session_mgr.lock().await;
                    sessions.set_state(session, CallState::Ended);
                }
                let mut active = self.active_session.lock().await;
                if *active == session {
                    *active = self.channel_session;
                }
                let _ = self
                    .events_tx
                    .send(MeshEvent::CallDeclined { session, from, reason })
                    .await;
            }
            CallControl::Bye { session, .. } => {
                {
                    let mut sessions = self.session_mgr.lock().await;
                    sessions.set_state(session, CallState::Ended);
                }
                let mut active = self.active_session.lock().await;
                if *active == session {
                    *active = self.channel_session;
                }
                let _ = self
                    .events_tx
                    .send(MeshEvent::CallEnded { session })
                    .await;
            }
            CallControl::Mute { .. } => {}
            CallControl::SessionInfo { session, participants } => {
                let mut sessions = self.session_mgr.lock().await;
                for peer in participants {
                    sessions.add_participant(session, peer);
                }
                drop(sessions);
                let _ = self.update_group_topology(session).await;
            }
        }
        Ok(())
    }

    async fn disconnect_addr(&self, addr: SocketAddr) {
        let peer_id = {
            let mut connections = self.connections.lock().await;
            connections.remove(&addr).and_then(|conn| conn.peer_id)
        };
        let Some(peer_id) = peer_id else {
            return;
        };
        let mut peers = self.peers_by_id.lock().await;
        peers.remove(&peer_id);
        drop(peers);
        let mut routes = self.routes.lock().await;
        routes.remove(&peer_id);
        drop(routes);
        let mut peer_addrs = self.peer_addrs.lock().await;
        peer_addrs.remove(&peer_id);
        drop(peer_addrs);
        let mut peer_candidates = self.peer_candidates.lock().await;
        peer_candidates.remove(&peer_id);
        drop(peer_candidates);
        let mut e2ee_keys = self.e2ee_keys.lock().await;
        e2ee_keys.retain(|(peer, _), _| *peer != peer_id);
        drop(e2ee_keys);
        let mut pending = self.e2ee_pending.lock().await;
        pending.retain(|(peer, _), _| *peer != peer_id);
        drop(pending);
        let _ = self.events_tx.send(MeshEvent::PeerLeft(peer_id)).await;
    }

    async fn socket_by_idx(&self, socket_idx: usize) -> Result<MeshSocket> {
        let sockets = self.sockets.lock().await;
        sockets
            .get(socket_idx)
            .cloned()
            .ok_or_else(|| anyhow!("missing socket"))
    }

    async fn send_raw(&self, socket_idx: usize, addr: SocketAddr, data: &[u8]) -> Result<()> {
        let socket = self.socket_by_idx(socket_idx).await?;
        match socket {
            MeshSocket::Udp(socket) => {
                socket.send_to(data, addr).await?;
                Ok(())
            }
            MeshSocket::Turn(relay) => {
                relay.send_to(addr, data).await?;
                Ok(())
            }
        }
    }

    async fn broadcast_voice(
        &self,
        _from: PeerId,
        seq: u32,
        timestamp: u64,
        payload: Vec<u8>,
    ) -> Result<()> {
        let codec = *self.group_codec.lock().await;
        let msg = RiftPayload::Voice(VoicePacket { codec_id: codec, payload });
        let session = *self.active_session.lock().await;
        let mode = self.current_group_mode(session).await;
        if let GroupMode::Hybrid { forwarder } = mode {
            if forwarder != self.identity.peer_id {
                let routes = self.routes_snapshot().await;
                if let Some(route) = routes.get(&forwarder).cloned() {
                    let _ = self
                        .send_to_peer(forwarder, route, msg, seq, timestamp, session)
                        .await;
                }
                return Ok(());
            }
        }

        let routes = self.routes_snapshot().await;
        for (peer_id, route) in routes {
            if peer_id == self.identity.peer_id {
                continue;
            }
            if let Err(err) = self
                .send_to_peer(peer_id, route, msg.clone(), seq, timestamp, session)
                .await
            {
                tracing::debug!(peer = %peer_id, "voice send failed: {err}");
            }
        }
        Ok(())
    }

    async fn routes_snapshot(&self) -> HashMap<PeerId, PeerRoute> {
        self.routes.lock().await.clone()
    }

    async fn send_to_peer(
        &self,
        peer_id: PeerId,
        route: PeerRoute,
        payload: RiftPayload,
        seq: u32,
        timestamp: u64,
        session: SessionId,
    ) -> Result<()> {
        match route {
            PeerRoute::Direct { addr } => self
                .send_payload(addr, payload, seq, timestamp, session, Some(peer_id))
                .await,
            PeerRoute::Relayed { via } => {
                let relay_addr = {
                    let peers = self.peers_by_id.lock().await;
                    peers.get(&via).copied()
                };
                let Some(relay_addr) = relay_addr else {
                    return Err(anyhow!("missing relay addr"));
                };
                let envelope = RiftPayload::Relay {
                    target: peer_id,
                    inner: Box::new(payload),
                };
                self.send_payload(relay_addr, envelope, seq, timestamp, session, Some(peer_id))
                    .await
            }
        }
    }

    async fn forward_relay(
        &self,
        target: PeerId,
        header: RiftFrameHeader,
        inner: RiftPayload,
    ) -> Result<()> {
        let route = {
            let routes = self.routes.lock().await;
            routes.get(&target).cloned()
        };
        let Some(route) = route else {
            return Ok(());
        };
        match route {
            PeerRoute::Direct { addr } => {
                let envelope = RiftPayload::Relay {
                    target,
                    inner: Box::new(inner),
                };
                self.send_payload_with_source(
                    addr,
                    envelope,
                    header.seq,
                    header.timestamp,
                    header.source,
                    header.session,
                    Some(target),
                )
                .await?;
            }
            PeerRoute::Relayed { via } => {
                let relay_addr = {
                    let peers = self.peers_by_id.lock().await;
                    peers.get(&via).copied()
                };
                if let Some(relay_addr) = relay_addr {
                    let envelope = RiftPayload::Relay {
                        target,
                        inner: Box::new(inner),
                    };
                    self.send_payload_with_source(
                        relay_addr,
                        envelope,
                        header.seq,
                        header.timestamp,
                        header.source,
                        header.session,
                        Some(target),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

impl MeshHandle {
    /// Broadcast a voice packet to all peers.
    pub async fn broadcast_voice(&self, seq: u32, timestamp: u64, payload: Vec<u8>) -> Result<()> {
        self.inner
            .broadcast_voice(self.inner.identity.peer_id, seq, timestamp, payload)
            .await
    }

    /// Broadcast a chat message to all peers.
    pub async fn broadcast_chat(&self, text: String) -> Result<()> {
        self.inner.broadcast_chat(text).await
    }

    /// Start a call with a specific peer.
    pub async fn start_call(&self, to: PeerId) -> Result<SessionId> {
        self.inner.start_call(to, None).await
    }

    /// Start a call with an optional rndzv SRT URI attached.
    pub async fn start_call_with_srt(
        &self,
        to: PeerId,
        rndzv_srt_uri: Option<String>,
    ) -> Result<SessionId> {
        self.inner.start_call(to, rndzv_srt_uri).await
    }

    /// Accept a pending call.
    pub async fn accept_call(&self, session: SessionId) -> Result<()> {
        self.inner.accept_call(session).await
    }

    /// Decline a pending call with an optional reason.
    pub async fn decline_call(&self, session: SessionId, reason: Option<String>) -> Result<()> {
        self.inner.decline_call(session, reason).await
    }

    /// End an active call.
    pub async fn end_call(&self, session: SessionId) -> Result<()> {
        self.inner.end_call(session).await
    }

    /// Update preferred codecs for capability negotiation.
    pub async fn set_preferred_codecs(&self, codecs: Vec<CodecId>) {
        self.inner.set_preferred_codecs(codecs).await;
    }

    /// Update preferred feature flags for capability negotiation.
    pub async fn set_preferred_features(&self, features: Vec<FeatureFlag>) {
        self.inner.set_preferred_features(features).await;
    }

    /// Retrieve the current session config for a peer.
    pub async fn peer_session_config(&self, peer_id: PeerId) -> Option<SessionConfig> {
        let sessions = self.inner.peer_session.lock().await;
        sessions.get(&peer_id).cloned()
    }

    /// Return the current group codec.
    pub async fn group_codec(&self) -> CodecId {
        *self.inner.group_codec.lock().await
    }

    /// Initiate a connection to a socket address.
    pub async fn connect_addr(&self, addr: SocketAddr) -> Result<()> {
        self.inner.initiate_handshake(addr, 0).await
    }

    /// Initiate a connection using a pre-bound socket.
    pub async fn connect_with_socket(&self, socket: UdpSocket, addr: SocketAddr) -> Result<()> {
        let socket_idx = self.inner.add_socket(socket).await?;
        self.inner.initiate_handshake(addr, socket_idx).await
    }

    /// Disconnect a peer and clean up any related routes.
    pub async fn disconnect_peer(&self, peer_id: PeerId) {
        let addr = {
            let peers = self.inner.peers_by_id.lock().await;
            peers.get(&peer_id).copied()
        };
        if let Some(addr) = addr {
            self.inner.disconnect_addr(addr).await;
        }
    }

    #[cfg(feature = "predictive-rendezvous")]
    /// Run Predictive Rendezvous over the mesh UDP socket.
    ///
    /// This drives slot emission and listens for matching probes. See
    /// `docs/predictive-rendezvous.md` for the conceptual model.
    pub async fn run_rendezvous(
        &self,
        cfg: RendezvousConfig,
    ) -> Result<RendezvousResult, RendezvousError> {
        let rendezvous_id = rendezvous_id_from_seed(&cfg.token.seed);
        let (tx, mut rx) = mpsc::channel::<(SocketAddr, ParsedProbe)>(64);
        self.inner
            .register_pr_session(rendezvous_id, tx)
            .await
            .map_err(RendezvousError::Mesh)?;

        struct PrGuard {
            inner: Arc<MeshInner>,
            rendezvous_id: u64,
        }

        impl Drop for PrGuard {
            fn drop(&mut self) {
                let inner = self.inner.clone();
                let rendezvous_id = self.rendezvous_id;
                tokio::spawn(async move {
                    inner.unregister_pr_session(rendezvous_id).await;
                });
            }
        }

        let _guard = PrGuard {
            inner: self.inner.clone(),
            rendezvous_id,
        };

        let mut state = RendezvousState::new(cfg.token.clone(), cfg.role);
        let mut last_sent_slot: Option<u64> = None;
        let mut last_local_offset: Option<u16> = None;
        let mut local_offsets = Vec::new();
        let mut metrics = RendezvousMetrics::new();
        let start_instant = TokioInstant::now();
        let deadline = TokioInstant::now() + cfg.max_duration;

        loop {
            if TokioInstant::now() >= deadline {
                metrics.total_duration_ms = start_instant.elapsed().as_millis() as u64;
                metrics.nat_behavior_hint = derive_nat_hint(&local_offsets);
                tracing::debug!(
                    target = "predictive_rendezvous",
                    rendezvous_id,
                    slots_attempted = metrics.slots_attempted,
                    probes_sent = metrics.probes_sent,
                    probes_received = metrics.probes_received,
                    "rendezvous timeout"
                );
                return Err(RendezvousError::Timeout);
            }

            let now_ms = now_timestamp();
            let t0_ms = cfg.token.time_model.t0.saturating_mul(1_000);
            let window_ms = cfg.token.time_model.window_secs.saturating_mul(1_000);
            let end_ms = t0_ms.saturating_add(window_ms);

            if cfg.token.time_model.slot_ms == 0 {
                return Err(RendezvousError::InvalidConfig("slot_ms must be > 0"));
            }
            if now_ms >= end_ms {
                metrics.total_duration_ms = start_instant.elapsed().as_millis() as u64;
                metrics.nat_behavior_hint = derive_nat_hint(&local_offsets);
                tracing::debug!(
                    target = "predictive_rendezvous",
                    rendezvous_id,
                    slots_attempted = metrics.slots_attempted,
                    "rendezvous window closed"
                );
                return Err(RendezvousError::WindowClosed);
            }

            if let Some(slot) = compute_slot_params(
                &cfg.token.seed,
                &cfg.token.time_model,
                cfg.role,
                now_ms,
            ) {
                if last_sent_slot != Some(slot.slot_index) {
                    last_sent_slot = Some(slot.slot_index);
                    last_local_offset = Some(slot.local_port_offset);
                    local_offsets.push(slot.local_port_offset);
                    metrics.slots_attempted = metrics.slots_attempted.saturating_add(1);
                    state.record_sent_slot(&slot);

                    tracing::debug!(
                        target = "predictive_rendezvous",
                        rendezvous_id,
                        slot_index = slot.slot_index,
                        local_port_offset = slot.local_port_offset,
                        remote_port_offset = slot.remote_port_offset,
                        "rendezvous slot emit"
                    );

                    let payload = build_probe_payload(ProbePayload {
                        rendezvous_id,
                        slot_index: slot.slot_index,
                        sender_fingerprint: cfg.sender_fingerprint,
                    });

                    for addr in &cfg.potential_remote_addrs {
                        self.inner
                            .send_raw(0, *addr, &payload)
                            .await
                            .map_err(RendezvousError::Mesh)?;
                        metrics.probes_sent = metrics.probes_sent.saturating_add(1);
                    }
                }
            }

            let next_wake_ms = if now_ms < t0_ms {
                t0_ms
            } else {
                let slot_index = (now_ms - t0_ms) / cfg.token.time_model.slot_ms;
                t0_ms + (slot_index + 1) * cfg.token.time_model.slot_ms
            };

            let sleep_ms = next_wake_ms.saturating_sub(now_ms).max(1);
            let wake_at = TokioInstant::now() + Duration::from_millis(sleep_ms);

            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    return Err(RendezvousError::Timeout);
                }
                _ = tokio::time::sleep_until(wake_at) => {
                    continue;
                }
                recv = rx.recv() => {
                    let Some((addr, probe)) = recv else {
                        metrics.total_duration_ms = start_instant.elapsed().as_millis() as u64;
                        metrics.nat_behavior_hint = derive_nat_hint(&local_offsets);
                        return Err(RendezvousError::ChannelClosed);
                    };
                    metrics.probes_received = metrics.probes_received.saturating_add(1);
                    let outcome = state.record_received_probe(addr, &probe);
                    if let RendezvousOutcome::Succeeded { remote_addr, slot_index } = outcome {
                        let base_port = self.inner.primary_local_port().await.map_err(RendezvousError::Mesh)?;
                        let local_port = last_local_offset
                            .map(|offset| base_port.wrapping_add(offset))
                            .unwrap_or(base_port);
                        metrics.slot_index_success = Some(slot_index);
                        metrics.total_duration_ms = start_instant.elapsed().as_millis() as u64;
                        metrics.nat_behavior_hint = derive_nat_hint(&local_offsets);
                        tracing::debug!(
                            target = "predictive_rendezvous",
                            rendezvous_id,
                            slot_index,
                            local_port,
                            "rendezvous succeeded"
                        );
                        return Ok(RendezvousResult {
                            remote_addr,
                            local_port,
                            slot_index,
                            metrics,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(feature = "predictive-rendezvous")]
/// Async rendezvous configuration for Predictive Rendezvous sessions.
#[derive(Debug, Clone)]
pub struct RendezvousConfig {
    pub token: SemanticRendezvousToken,
    pub role: Role,
    pub potential_remote_addrs: Vec<SocketAddr>,
    pub max_duration: Duration,
    pub sender_fingerprint: [u8; 16],
}

#[cfg(feature = "predictive-rendezvous")]
/// Result of a successful rendezvous attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendezvousResult {
    pub remote_addr: SocketAddr,
    pub local_port: u16,
    pub slot_index: u64,
    pub metrics: RendezvousMetrics,
}

#[cfg(feature = "predictive-rendezvous")]
#[derive(Debug)]
pub enum RendezvousError {
    Timeout,
    WindowClosed,
    ChannelClosed,
    InvalidConfig(&'static str),
    Mesh(anyhow::Error),
}

#[cfg(feature = "predictive-rendezvous")]
impl std::fmt::Display for RendezvousError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendezvousError::Timeout => write!(f, "rendezvous timed out"),
            RendezvousError::WindowClosed => write!(f, "rendezvous window closed"),
            RendezvousError::ChannelClosed => write!(f, "rendezvous channel closed"),
            RendezvousError::InvalidConfig(msg) => write!(f, "invalid rendezvous config: {msg}"),
            RendezvousError::Mesh(err) => write!(f, "mesh error: {err}"),
        }
    }
}

#[cfg(feature = "predictive-rendezvous")]
impl std::error::Error for RendezvousError {}

#[cfg(feature = "predictive-rendezvous")]
fn derive_nat_hint(offsets: &[u16]) -> rift_rndzv::NatBehaviorHint {
    if offsets.len() < 2 {
        return rift_rndzv::NatBehaviorHint::Unknown;
    }
    let mut unique = std::collections::HashSet::new();
    for offset in offsets {
        unique.insert(*offset);
    }
    if unique.len() == 1 {
        rift_rndzv::NatBehaviorHint::PortPreserving
    } else {
        rift_rndzv::NatBehaviorHint::HighVariance
    }
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn stream_for_payload(payload: &RiftPayload) -> StreamKind {
    match payload {
        RiftPayload::Control(_) => StreamKind::Control,
        RiftPayload::Voice(_) => StreamKind::Voice,
        RiftPayload::Text(_) => StreamKind::Text,
        RiftPayload::Relay { inner, .. } => stream_for_payload(inner),
        RiftPayload::Encrypted(_) => StreamKind::Control,
    }
}

fn should_encrypt(payload: &RiftPayload) -> bool {
    match payload {
        RiftPayload::Voice(_) => true,
        RiftPayload::Text(_) => true,
        RiftPayload::Control(ControlMessage::Chat(_)) => true,
        RiftPayload::Relay { .. } => false,
        RiftPayload::Encrypted(_) => false,
        RiftPayload::Control(_) => false,
    }
}

fn encrypt_payload_with_key(
    key: &[u8; 32],
    header: &RiftFrameHeader,
    payload: &RiftPayload,
) -> Result<EncryptedPayload> {
    let plaintext = bincode::serialize(payload)?;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = bincode::serialize(header)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: &plaintext, aad: &aad })
        .map_err(|_| anyhow!("e2ee encrypt failed"))?;
    Ok(EncryptedPayload { nonce, ciphertext })
}

#[cfg(all(test, feature = "predictive-rendezvous"))]
mod pr_tests {
    use super::*;
    use rift_rndzv::{EscalationPolicy, IdentityConstraints, SearchStrategy, SemanticRendezvousToken, TimeModel};

    #[tokio::test]
    async fn predictive_rendezvous_succeeds_loopback() {
        let cfg = MeshConfig {
            channel_name: "pr-test".to_string(),
            password: None,
            listen_port: 0,
            relay_capable: false,
            qos: QosProfile::default(),
            auth_token: None,
            require_auth: false,
            e2ee_key: None,
            rekey_interval_secs: None,
            max_direct_peers: None,
        };

        let mesh_a = Mesh::new(Identity::generate(), cfg.clone()).await.unwrap();
        let mesh_b = Mesh::new(Identity::generate(), cfg.clone()).await.unwrap();

        let addr_a = mesh_a.local_addr().await.unwrap();
        let addr_b = mesh_b.local_addr().await.unwrap();

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let token = SemanticRendezvousToken::new(
            rift_rndzv::api::RendezvousSpaceId([0u8; 32]),
            [7u8; 32],
            IdentityConstraints {
                allowed_fingerprints: Vec::new(),
            },
            TimeModel {
                t0: now_secs + 1,
                window_secs: 3,
                slot_ms: 200,
            },
            SearchStrategy::BasicDeterministic,
            EscalationPolicy::None,
        );

        let cfg_a = RendezvousConfig {
            token: token.clone(),
            role: Role::Caller,
            potential_remote_addrs: vec![addr_b],
            max_duration: Duration::from_secs(3),
            sender_fingerprint: [1u8; 16],
        };

        let cfg_b = RendezvousConfig {
            token,
            role: Role::Callee,
            potential_remote_addrs: vec![addr_a],
            max_duration: Duration::from_secs(3),
            sender_fingerprint: [2u8; 16],
        };

        let handle_a = mesh_a.handle();
        let handle_b = mesh_b.handle();

        let (res_a, res_b) = tokio::join!(
            handle_a.run_rendezvous(cfg_a),
            handle_b.run_rendezvous(cfg_b),
        );

        assert!(res_a.is_ok(), "caller rendezvous failed: {:?}", res_a);
        assert!(res_b.is_ok(), "callee rendezvous failed: {:?}", res_b);
    }
}

fn decrypt_payload_with_key(
    key: &[u8; 32],
    header: &RiftFrameHeader,
    encrypted: EncryptedPayload,
) -> Result<RiftPayload> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = bincode::serialize(header)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&encrypted.nonce), Payload { msg: &encrypted.ciphertext, aad: &aad })
        .map_err(|_| anyhow!("e2ee decrypt failed"))?;
    let payload: RiftPayload = bincode::deserialize(&plaintext)?;
    Ok(payload)
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn e2ee_encrypt_decrypt_roundtrip() {
        let key = [7u8; 32];
        let header = RiftFrameHeader {
            version: ProtocolVersion::V1,
            stream: StreamKind::Voice,
            flags: 0,
            seq: 1,
            timestamp: 123,
            source: PeerId([1u8; 32]),
            session: SessionId::NONE,
        };
        let payload = RiftPayload::Voice(VoicePacket {
            codec_id: CodecId::Opus,
            payload: vec![1, 2, 3, 4],
        });

        let encrypted = encrypt_payload_with_key(&key, &header, &payload).unwrap();
        let decoded = decrypt_payload_with_key(&key, &header, encrypted).unwrap();
        match decoded {
            RiftPayload::Voice(pkt) => assert_eq!(pkt.payload, vec![1, 2, 3, 4]),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn e2ee_aad_mismatch_fails() {
        let key = [9u8; 32];
        let mut header = RiftFrameHeader {
            version: ProtocolVersion::V1,
            stream: StreamKind::Text,
            flags: 0,
            seq: 5,
            timestamp: 999,
            source: PeerId([2u8; 32]),
            session: SessionId::NONE,
        };
        let payload = RiftPayload::Text(ChatMessage::new(
            PeerId([3u8; 32]),
            999,
            "hi".to_string(),
        ));

        let encrypted = encrypt_payload_with_key(&key, &header, &payload).unwrap();
        header.seq = 6;
        let res = decrypt_payload_with_key(&key, &header, encrypted);
        assert!(res.is_err());
    }
}

fn default_peer_capabilities() -> Capabilities {
    Capabilities {
        supported_versions: vec![ProtocolVersion::V1],
        audio_codecs: vec![CodecId::Opus],
        features: vec![FeatureFlag::Voice, FeatureFlag::Text],
        max_bitrate: Some(48_000),
        preferred_frame_duration_ms: Some(20),
    }
}
