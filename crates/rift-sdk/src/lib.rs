//! Rift SDK: high-level API for embedding Rift VoIP in other applications.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;

use rift_core::{decode_invite, generate_invite, Identity, Invite, PeerId, KeyStore};
use rift_dht::{DhtConfig as RiftDhtConfig, DhtHandle, PeerEndpointInfo};
use rift_discovery::local_ipv4_addrs;
use rift_media::{
    decode_frame, encode_frame, AudioConfig, AudioIn, AudioMixer, AudioOut, OpusDecoder,
    OpusEncoder,
};
use rift_mesh::{Mesh, MeshConfig, MeshEvent, MeshHandle};
use rift_nat::{attempt_hole_punch, gather_public_addrs, NatConfig, PeerEndpoint};
use rift_protocol::{CallState, Capabilities, QosProfile, SessionId};
use hkdf::Hkdf;
use sha2::Sha256;

pub use rift_core::PeerId as RiftPeerId;
pub use rift_protocol::{
    CallState as RiftCallState, ChatMessage, CodecId, FeatureFlag, QosProfile as RiftQosProfile,
    SessionId as RiftSessionId,
};

pub const SDK_VERSION: &str = "0.1.0";
pub const SDK_ABI_VERSION: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftConfig {
    pub identity_path: Option<PathBuf>,
    pub listen_port: u16,
    pub relay: bool,
    pub user_name: Option<String>,
    pub preferred_codecs: Vec<CodecId>,
    pub preferred_features: Vec<FeatureFlag>,
    #[serde(default)]
    pub qos: QosProfile,
    #[serde(default)]
    pub metrics_enabled: bool,
    #[serde(default)]
    pub security: SecurityConfig,
    pub dht: DhtConfigSdk,
    pub audio: AudioConfigSdk,
    pub network: NetworkConfigSdk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfigSdk {
    pub enabled: bool,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub quality: String,
    pub ptt: bool,
    pub vad: bool,
    pub mute_output: bool,
    pub emit_voice_frames: bool,
    pub allow_fail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfigSdk {
    pub prefer_p2p: bool,
    pub local_ports: Option<Vec<u16>>,
    pub known_peers: Vec<std::net::SocketAddr>,
    pub invite: Option<String>,
    #[serde(default)]
    pub stun_servers: Vec<String>,
    #[serde(default)]
    pub stun_timeout_ms: Option<u64>,
    #[serde(default)]
    pub punch_interval_ms: Option<u64>,
    #[serde(default)]
    pub punch_timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_direct_peers: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtConfigSdk {
    pub enabled: bool,
    pub bootstrap_nodes: Vec<String>,
    pub listen_addr: Option<String>,
}

impl Default for RiftConfig {
    fn default() -> Self {
        Self {
            identity_path: None,
            listen_port: 7777,
            relay: false,
            user_name: None,
            preferred_codecs: vec![CodecId::Opus, CodecId::PCM16],
            preferred_features: vec![
                FeatureFlag::Voice,
                FeatureFlag::Text,
                FeatureFlag::Relay,
                FeatureFlag::E2EE,
            ],
            qos: QosProfile::default(),
            metrics_enabled: true,
            security: SecurityConfig::default(),
            dht: DhtConfigSdk::default(),
            audio: AudioConfigSdk::default(),
            network: NetworkConfigSdk::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub trust_on_first_use: bool,
    pub known_hosts_path: Option<PathBuf>,
    pub reject_on_mismatch: bool,
    pub channel_shared_secret: Option<String>,
    pub audit_log_path: Option<PathBuf>,
    pub rekey_interval_secs: Option<u64>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            trust_on_first_use: true,
            known_hosts_path: None,
            reject_on_mismatch: false,
            channel_shared_secret: None,
            audit_log_path: None,
            rekey_interval_secs: Some(600),
        }
    }
}

impl Default for AudioConfigSdk {
    fn default() -> Self {
        Self {
            enabled: true,
            input_device: None,
            output_device: None,
            quality: "medium".to_string(),
            ptt: false,
            vad: true,
            mute_output: false,
            emit_voice_frames: false,
            allow_fail: false,
        }
    }
}

impl Default for NetworkConfigSdk {
    fn default() -> Self {
        Self {
            prefer_p2p: true,
            local_ports: None,
            known_peers: Vec::new(),
            invite: None,
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
            ],
            stun_timeout_ms: Some(800),
            punch_interval_ms: Some(200),
            punch_timeout_ms: Some(5000),
            max_direct_peers: None,
        }
    }
}

impl Default for DhtConfigSdk {
    fn default() -> Self {
        Self {
            enabled: false,
            bootstrap_nodes: Vec::new(),
            listen_addr: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinkStats {
    pub rtt_ms: f32,
    pub loss: f32,
    pub jitter_ms: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalStats {
    pub num_peers: usize,
    pub num_sessions: usize,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Debug, Clone)]
pub enum RouteKind {
    Direct,
    Relayed { via: PeerId },
}

#[derive(Debug, Clone)]
pub enum RiftEvent {
    IncomingChat(ChatMessage),
    IncomingCall { session: SessionId, from: PeerId },
    CallStateChanged { session: SessionId, state: CallState },
    PeerJoinedChannel { peer: PeerId, channel: String },
    PeerLeftChannel { peer: PeerId, channel: String },
    PeerCapabilities { peer: PeerId, capabilities: Capabilities },
    AudioLevel { peer: PeerId, level: f32 },
    CodecSelected { codec: CodecId },
    AudioBitrate { bitrate: u32 },
    StatsUpdate { peer: PeerId, stats: LinkStats, global: GlobalStats },
    RouteUpdated { peer: PeerId, route: RouteKind },
    SecurityNotice { message: String },
    VoiceFrame { peer: PeerId, samples: Vec<i16> },
}

#[derive(Debug, thiserror::Error)]
pub enum RiftError {
    #[error("not initialized")]
    NotInitialized,
    #[error("channel already joined")]
    AlreadyJoined,
    #[error("channel not joined")]
    NotJoined,
    #[error("mesh error: {0}")]
    Mesh(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("other: {0}")]
    Other(String),
}

struct VoiceRuntime {
    _audio_in: AudioIn,
    mixer: Arc<StdMutex<AudioMixer>>,
    frame_samples: usize,
    emit_voice: bool,
    audio_config: AudioConfig,
    tuning: Arc<StdMutex<AudioTuning>>,
}

#[derive(Debug, Clone)]
struct AudioTuning {
    bitrate: u32,
    fec: bool,
    loss_pct: u8,
}

struct QosState {
    profile: QosProfile,
    peer_stats: HashMap<PeerId, LinkStats>,
    current: AudioTuning,
    last_adjust: Instant,
}

struct SessionRuntime {
    _channel: String,
    handle: MeshHandle,
    _voice: Option<VoiceRuntime>,
    _dht: Option<DhtHandle>,
}

pub struct RiftHandle {
    identity: Mutex<Option<Identity>>,
    local_peer_id: PeerId,
    config: RiftConfig,
    overrides: Mutex<RiftConfigOverrides>,
    runtime: Mutex<Option<SessionRuntime>>,
    event_rx: Mutex<mpsc::UnboundedReceiver<RiftEvent>>,
    event_tx: mpsc::UnboundedSender<RiftEvent>,
    ptt_active: Arc<AtomicBool>,
    mute_active: Arc<AtomicBool>,
}

#[derive(Debug, Default, Clone)]
struct RiftConfigOverrides {
    dht_enabled: Option<bool>,
    bootstrap_nodes: Option<Vec<String>>,
    invite: Option<String>,
}

impl RiftHandle {
    pub async fn new(config: RiftConfig) -> Result<Self, RiftError> {
        rift_metrics::set_enabled(config.metrics_enabled);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let identity_path = config
            .identity_path
            .clone()
            .unwrap_or_else(|| Identity::default_path().unwrap_or_else(|_| PathBuf::from("identity.key")));
        let existed = identity_path.exists();
        let identity = KeyStore::load_or_generate(&identity_path)
            .context("identity load failed")
            .map_err(|e| RiftError::Other(format!("{e}")))?;
        if !existed {
            tracing::info!(path = %identity_path.display(), "new identity generated");
        }
        let local_peer_id = identity.peer_id;
        Ok(Self {
            ptt_active: Arc::new(AtomicBool::new(!config.audio.ptt)),
            identity: Mutex::new(Some(identity)),
            local_peer_id,
            config,
            overrides: Mutex::new(RiftConfigOverrides::default()),
            runtime: Mutex::new(None),
            event_rx: Mutex::new(event_rx),
            event_tx,
            mute_active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn set_dht_enabled(&self, enabled: bool) {
        let mut overrides = self.overrides.lock().await;
        overrides.dht_enabled = Some(enabled);
    }

    pub async fn set_bootstrap_nodes(&self, nodes: Vec<String>) {
        let mut overrides = self.overrides.lock().await;
        overrides.bootstrap_nodes = Some(nodes);
    }

    pub async fn set_invite(&self, invite: Option<String>) {
        let mut overrides = self.overrides.lock().await;
        overrides.invite = invite;
    }

    pub async fn join_channel(
        &self,
        name: &str,
        password: Option<&str>,
        internet: bool,
    ) -> Result<(), RiftError> {
        let mut cfg = self.config.clone();
        {
            let overrides = self.overrides.lock().await;
            if let Some(enabled) = overrides.dht_enabled {
                cfg.dht.enabled = enabled;
            }
            if let Some(nodes) = overrides.bootstrap_nodes.clone() {
                cfg.dht.bootstrap_nodes = nodes;
            }
            if let Some(invite) = overrides.invite.clone() {
                cfg.network.invite = Some(invite);
            }
        }
        let mut runtime_guard = self.runtime.lock().await;
        if runtime_guard.is_some() {
            return Err(RiftError::AlreadyJoined);
        }
        let identity = {
            let mut identity_guard = self.identity.lock().await;
            match identity_guard.take() {
                Some(identity) => identity,
                None => Identity::load(cfg.identity_path.as_deref())
                    .context("identity not found")
                    .map_err(|e| RiftError::Other(format!("{e}")))?,
            }
        };

        let auth_token = self
            .config
            .security
            .channel_shared_secret
            .as_deref()
            .map(|secret| derive_auth_token(secret, name));
        let nat_cfg = if internet {
            Some(default_nat_config(
                cfg.listen_port,
                cfg.network.local_ports.clone(),
                cfg.network.stun_servers.clone(),
                cfg.network.stun_timeout_ms,
                cfg.network.punch_interval_ms,
                cfg.network.punch_timeout_ms,
            ))
        } else {
            None
        };
        let mut known_peers = cfg.network.known_peers.clone();
        if internet && known_peers.is_empty() {
            if let Some(nat_cfg) = nat_cfg.as_ref() {
                if let Ok(public_addrs) = gather_public_addrs(nat_cfg).await {
                    if !public_addrs.is_empty() {
                        known_peers = public_addrs;
                    }
                }
            }
        }
        let invite_for_key = if let Some(invite_str) = &cfg.network.invite {
            Some(decode_invite(invite_str).map_err(|e| RiftError::Other(format!("{e}")))?)
        } else if internet {
            Some(generate_invite(name, password, known_peers.clone()))
        } else {
            None
        };
        let e2ee_key = derive_e2ee_key(
            name,
            password,
            invite_for_key.as_ref(),
            cfg.security.channel_shared_secret.as_deref(),
        );
        let config = MeshConfig {
            channel_name: name.to_string(),
            password: password.map(|v| v.to_string()),
            listen_port: cfg.listen_port,
            relay_capable: cfg.relay,
            qos: cfg.qos.clone(),
            auth_token,
            require_auth: cfg.security.channel_shared_secret.is_some(),
            e2ee_key,
            rekey_interval_secs: cfg.security.rekey_interval_secs,
            max_direct_peers: cfg.network.max_direct_peers,
        };
        let mut mesh = Mesh::new(identity, config)
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))?;

        let handle = mesh.handle();
        handle
            .set_preferred_codecs(self.config.preferred_codecs.clone())
            .await;
        handle
            .set_preferred_features(self.config.preferred_features.clone())
            .await;
        let security_handle = handle.clone();

        if internet {
            let nat_cfg = nat_cfg.clone().expect("nat cfg");
            mesh.enable_nat(nat_cfg.clone()).await;
            let invite = invite_for_key.clone().unwrap_or_else(|| Invite {
                channel_name: name.to_string(),
                password: password.map(|v| v.to_string()),
                channel_key: [0u8; 32],
                known_peers: Vec::new(),
                version: 1,
                created_at: now_timestamp(),
            });
            mesh.join_invite(invite, nat_cfg)
                .await
                .map_err(|e| RiftError::Mesh(format!("{e}")))?;
        } else {
            mesh.start_lan_discovery()
                .map_err(|e| RiftError::Mesh(format!("{e}")))?;
        }

        let event_tx = self.event_tx.clone();
        let channel = name.to_string();
        let channel_for_task = channel.clone();
        let voice = if cfg.audio.enabled {
            match start_audio_pipeline(
                cfg.clone(),
                handle.clone(),
                self.ptt_active.clone(),
                self.mute_active.clone(),
            ) {
                Ok(voice) => Some(voice),
                Err(err) => {
                    if cfg.audio.allow_fail {
                        tracing::warn!("audio pipeline failed: {err}");
                        None
                    } else {
                        return Err(err);
                    }
                }
            }
        } else {
            None
        };
        if let Some(voice) = voice.as_ref() {
            let _ = event_tx.send(RiftEvent::AudioBitrate {
                bitrate: voice.audio_config.bitrate,
            });
        }

        let dht = if cfg.dht.enabled {
            let dht_config = RiftDhtConfig {
                bootstrap_nodes: parse_socket_addrs(&cfg.dht.bootstrap_nodes),
                listen_addr: cfg
                    .dht
                    .listen_addr
                    .as_deref()
                    .and_then(parse_socket_addr)
                    .unwrap_or_else(|| {
                        SocketAddr::from(([0, 0, 0, 0], cfg.listen_port.saturating_add(100)))
                    }),
            };
            let handle_dht = DhtHandle::new(dht_config)
                .await
                .map_err(|e| RiftError::Other(format!("{e}")))?;

            let channel_id = rift_core::ChannelId::from_channel(&channel, password);
            let nat_cfg = nat_cfg.clone().unwrap_or_else(|| default_nat_config(
                cfg.listen_port,
                cfg.network.local_ports.clone(),
                cfg.network.stun_servers.clone(),
                cfg.network.stun_timeout_ms,
                cfg.network.punch_interval_ms,
                cfg.network.punch_timeout_ms,
            ));
            let addrs = match gather_public_addrs(&nat_cfg).await {
                Ok(public_addrs) if !public_addrs.is_empty() => public_addrs,
                _ => local_ipv4_addrs()
                    .map_err(|e| RiftError::Other(format!("{e}")))?
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip, cfg.listen_port))
                    .collect::<Vec<_>>(),
            };
            let info = PeerEndpointInfo {
                peer_id: self.local_peer_id,
                addrs,
            };
            let _ = handle_dht.announce(channel_id, info.clone()).await;

            let announce_handle = handle_dht.clone();
            let announce_info = info.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(30));
                loop {
                    tick.tick().await;
                    let _ = announce_handle
                        .announce(channel_id, announce_info.clone())
                        .await;
                }
            });

            let lookup_handle = handle_dht.clone();
            let mesh_handle = handle.clone();
            let nat_cfg = default_nat_config(
                self.config.listen_port,
                self.config.network.local_ports.clone(),
                self.config.network.stun_servers.clone(),
                self.config.network.stun_timeout_ms,
                self.config.network.punch_interval_ms,
                self.config.network.punch_timeout_ms,
            );
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(12));
                loop {
                    tick.tick().await;
                    if let Ok(peers) = lookup_handle.lookup(channel_id).await {
                        for peer in peers {
                            if peer.peer_id == info.peer_id {
                                continue;
                            }
                            for addr in peer.addrs.iter().copied() {
                                let endpoint = PeerEndpoint {
                                    peer_id: peer.peer_id,
                                    external_addrs: vec![addr],
                                    punch_ports: vec![addr.port()],
                                };
                                if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                                    let _ = mesh_handle.connect_with_socket(socket, remote).await;
                                } else {
                                    let _ = mesh_handle.connect_addr(addr).await;
                                }
                            }
                        }
                    }
                }
            });

            Some(handle_dht)
        } else {
            None
        };

        let voice_state = voice.as_ref().map(|v| VoiceRuntimeRef {
            mixer: v.mixer.clone(),
            frame_samples: v.frame_samples,
            emit_voice: v.emit_voice,
            audio_config: v.audio_config.clone(),
            tuning: v.tuning.clone(),
        });
        let security_cfg = self.config.security.clone();
        let qos_profile = self.config.qos.clone();
        let mut qos_state = voice_state.as_ref().map(|state| QosState {
            profile: qos_profile,
            peer_stats: HashMap::new(),
            current: AudioTuning {
                bitrate: state.audio_config.bitrate,
                fec: false,
                loss_pct: 0,
            },
            last_adjust: Instant::now() - Duration::from_secs(5),
        });

        tokio::spawn(async move {
            let mut mesh = mesh;
            let mut decoder = if voice_state.is_some() {
                Some(
                    OpusDecoder::new(&voice_state.as_ref().unwrap().audio_config)
                        .expect("opus decoder"),
                )
            } else {
                None
            };
            while let Some(event) = mesh.next_event().await {
                match event {
                    MeshEvent::PeerJoined(peer) => {
                        let _ = event_tx.send(RiftEvent::PeerJoinedChannel {
                            peer,
                            channel: channel_for_task.clone(),
                        });
                    }
                    MeshEvent::PeerLeft(peer) => {
                        let _ = event_tx.send(RiftEvent::PeerLeftChannel {
                            peer,
                            channel: channel_for_task.clone(),
                        });
                    }
                    MeshEvent::ChatReceived(chat) => {
                        let _ = event_tx.send(RiftEvent::IncomingChat(chat));
                    }
                    MeshEvent::IncomingCall { session, from } => {
                        let _ = event_tx.send(RiftEvent::IncomingCall { session, from });
                    }
                    MeshEvent::CallAccepted { session, .. } => {
                        let _ = event_tx.send(RiftEvent::CallStateChanged {
                            session,
                            state: CallState::Active,
                        });
                    }
                    MeshEvent::CallDeclined { session, .. } => {
                        let _ = event_tx.send(RiftEvent::CallStateChanged {
                            session,
                            state: CallState::Ended,
                        });
                    }
                    MeshEvent::CallEnded { session } => {
                        let _ = event_tx.send(RiftEvent::CallStateChanged {
                            session,
                            state: CallState::Ended,
                        });
                    }
                    MeshEvent::VoiceFrame { from, codec, payload, .. } => {
                        if let (Some(state), Some(decoder)) = (voice_state.as_ref(), decoder.as_mut()) {
                            if let Ok(out) = decode_frame(codec, &payload, decoder, state.frame_samples) {
                                let mut mixer = state.mixer.lock().unwrap();
                                mixer.push(peer_to_stream_id(&from), out.clone());
                                let level = audio_level(&out);
                                let _ = event_tx.send(RiftEvent::AudioLevel { peer: from, level });
                                if state.emit_voice {
                                    let _ = event_tx.send(RiftEvent::VoiceFrame { peer: from, samples: out });
                                }
                            }
                        }
                    }
                    MeshEvent::PeerCapabilities { peer_id, capabilities } => {
                        let _ = event_tx.send(RiftEvent::PeerCapabilities { peer: peer_id, capabilities });
                    }
                    MeshEvent::GroupCodec(codec) => {
                        let _ = event_tx.send(RiftEvent::CodecSelected { codec });
                    }
                    MeshEvent::StatsUpdate { peer, stats, global } => {
                        let sdk_stats = LinkStats {
                            rtt_ms: stats.rtt_ms,
                            loss: stats.loss,
                            jitter_ms: stats.jitter_ms,
                        };
                        let sdk_global = GlobalStats {
                            num_peers: global.num_peers,
                            num_sessions: global.num_sessions,
                            packets_sent: global.packets_sent,
                            packets_received: global.packets_received,
                            bytes_sent: global.bytes_sent,
                            bytes_received: global.bytes_received,
                        };
                        let _ = event_tx.send(RiftEvent::StatsUpdate {
                            peer,
                            stats: sdk_stats,
                            global: sdk_global,
                        });
                        if let (Some(state), Some(qos)) = (voice_state.as_ref(), qos_state.as_mut()) {
                            qos.peer_stats.insert(peer, sdk_stats);
                            if let Some(next) = compute_next_tuning(qos) {
                                let mut tuning = state.tuning.lock().unwrap();
                                let bitrate_changed = tuning.bitrate != next.bitrate;
                                *tuning = next.clone();
                                if bitrate_changed {
                                    let _ = event_tx.send(RiftEvent::AudioBitrate { bitrate: next.bitrate });
                                }
                            }
                        }
                    }
                    MeshEvent::RouteUpdated { peer_id, route } => {
                        let route = match route {
                            rift_mesh::PeerRoute::Direct { .. } => RouteKind::Direct,
                            rift_mesh::PeerRoute::Relayed { via } => RouteKind::Relayed { via },
                        };
                        let _ = event_tx.send(RiftEvent::RouteUpdated {
                            peer: peer_id,
                            route,
                        });
                    }
                    MeshEvent::PeerIdentity { peer_id, public_key } => {
                        if let Err(err) = handle_peer_identity(
                            &event_tx,
                            &security_handle,
                            &security_cfg,
                            peer_id,
                            &public_key,
                        )
                        .await
                        {
                            tracing::warn!("security check failed: {err}");
                        }
                    }
                    MeshEvent::PeerSessionConfig { .. } | MeshEvent::RouteUpgraded(_) => {}
                }
            }
        });

        *runtime_guard = Some(SessionRuntime {
            _channel: channel,
            handle,
            _voice: voice,
            _dht: dht,
        });
        Ok(())
    }

    pub async fn leave_channel(&self, _name: &str) -> Result<(), RiftError> {
        let mut runtime_guard = self.runtime.lock().await;
        if runtime_guard.is_none() {
            return Err(RiftError::NotJoined);
        }
        *runtime_guard = None;
        Ok(())
    }

    pub async fn send_chat(&self, text: &str) -> Result<(), RiftError> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or(RiftError::NotJoined)?;
        runtime
            .handle
            .broadcast_chat(text.to_string())
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))
    }

    pub async fn start_call(&self, peer: PeerId) -> Result<SessionId, RiftError> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or(RiftError::NotJoined)?;
        runtime
            .handle
            .start_call(peer)
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))
    }

    pub async fn accept_call(&self, session: SessionId) -> Result<(), RiftError> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or(RiftError::NotJoined)?;
        runtime
            .handle
            .accept_call(session)
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))
    }

    pub async fn decline_call(&self, session: SessionId, reason: Option<&str>) -> Result<(), RiftError> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or(RiftError::NotJoined)?;
        runtime
            .handle
            .decline_call(session, reason.map(|v| v.to_string()))
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))
    }

    pub async fn end_call(&self, session: SessionId) -> Result<(), RiftError> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or(RiftError::NotJoined)?;
        runtime
            .handle
            .end_call(session)
            .await
            .map_err(|e| RiftError::Mesh(format!("{e}")))
    }

    pub async fn next_event(&self) -> Option<RiftEvent> {
        let mut rx = self.event_rx.lock().await;
        rx.recv().await
    }

    pub fn try_next_event(&self) -> Option<RiftEvent> {
        let mut rx = self.event_rx.blocking_lock();
        rx.try_recv().ok()
    }

    pub fn set_ptt_active(&self, active: bool) {
        self.ptt_active.store(active, Ordering::Relaxed);
    }

    pub fn set_mute(&self, muted: bool) {
        self.mute_active.store(muted, Ordering::Relaxed);
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }
}

struct VoiceRuntimeRef {
    mixer: Arc<StdMutex<AudioMixer>>,
    frame_samples: usize,
    emit_voice: bool,
    audio_config: AudioConfig,
    tuning: Arc<StdMutex<AudioTuning>>,
}

fn start_audio_pipeline(
    config: RiftConfig,
    handle: MeshHandle,
    ptt_active: Arc<AtomicBool>,
    mute_active: Arc<AtomicBool>,
) -> Result<VoiceRuntime, RiftError> {
    let mut audio_config = AudioConfig::default();
    let initial_bitrate = map_quality_to_bitrate(Some(&config.audio.quality));
    audio_config.bitrate = initial_bitrate
        .clamp(config.qos.min_bitrate, config.qos.max_bitrate)
        .max(8_000);
    rift_metrics::set_gauge("rift_audio_bitrate", &[], audio_config.bitrate as f64);
    let (audio_in, mut audio_rx) = AudioIn::new_with_device(&audio_config, config.audio.input_device.as_deref())
        .map_err(|e| RiftError::Audio(format!("{e}")))?;
    let mut encoder = OpusEncoder::new(&audio_config).map_err(|e| RiftError::Audio(format!("{e}")))?;
    let output_device = config.audio.output_device.clone();
    let mixer = Arc::new(StdMutex::new(AudioMixer::with_prebuffer(
        audio_config.frame_samples(),
        8,
    )));

    let ptt_enabled = config.audio.ptt;
    let vad_enabled = config.audio.vad;
    let frame_duration = audio_config.frame_duration();
    let tuning = Arc::new(StdMutex::new(AudioTuning {
        bitrate: audio_config.bitrate,
        fec: false,
        loss_pct: 0,
    }));
    let tuning_for_task = tuning.clone();
    tokio::spawn(async move {
        let mut seq: u32 = 0;
        let mut hangover: u8 = 0;
        let mut last_applied = AudioTuning {
            bitrate: audio_config.bitrate,
            fec: false,
            loss_pct: 0,
        };
        while let Some(frame) = audio_rx.recv().await {
            let next_tuning = {
                let tuning = tuning_for_task.lock().unwrap();
                tuning.clone()
            };
            if next_tuning.bitrate != last_applied.bitrate
                || next_tuning.fec != last_applied.fec
                || next_tuning.loss_pct != last_applied.loss_pct
            {
                if let Err(err) = encoder.set_bitrate(next_tuning.bitrate) {
                    tracing::debug!("opus bitrate update failed: {err}");
                }
                if let Err(err) = encoder.set_fec(next_tuning.fec) {
                    tracing::debug!("opus fec update failed: {err}");
                }
                if let Err(err) = encoder.set_packet_loss(next_tuning.loss_pct) {
                    tracing::debug!("opus loss update failed: {err}");
                }
                last_applied = next_tuning;
            }
            if ptt_enabled && !ptt_active.load(Ordering::Relaxed) {
                continue;
            }
            if mute_active.load(Ordering::Relaxed) {
                continue;
            }
            if !ptt_enabled && vad_enabled {
                let active = is_frame_active(&frame);
                if active {
                    hangover = 4;
                } else if hangover > 0 {
                    hangover -= 1;
                }
                if !active && hangover == 0 {
                    continue;
                }
            }
            if frame_duration > Duration::from_millis(20) {
                tokio::time::sleep(frame_duration - Duration::from_millis(20)).await;
            }
            let codec = handle.group_codec().await;
            let out = match encode_frame(codec, &frame, &mut encoder) {
                Ok(out) => out,
                Err(_) => continue,
            };
            let timestamp = now_timestamp();
            let _ = handle.broadcast_voice(seq, timestamp, out).await;
            seq = seq.wrapping_add(1);
        }
    });

    if !config.audio.mute_output {
        let mixer = mixer.clone();
        let audio_config = audio_config.clone();
        std::thread::spawn(move || {
            let audio_out = match AudioOut::new_with_device(&audio_config, output_device.as_deref()) {
                Ok(out) => out,
                Err(err) => {
                    tracing::warn!("audio output init failed: {err}");
                    return;
                }
            };
            let frame_samples = audio_out.frame_samples();
            let frame_duration = audio_config.frame_duration();
            let target_frames = 6usize;
            let mut last_frame = vec![0i16; frame_samples];
            let mut last_active = Instant::now() - Duration::from_secs(1);
            loop {
                std::thread::sleep(frame_duration);
                while audio_out.queued_samples() < target_frames * frame_samples {
                    let (frame, active) = {
                        let mut mixer = mixer.lock().unwrap();
                        mixer.mix_next_with_activity()
                    };
                    let out_frame = if active {
                        last_active = Instant::now();
                        last_frame.clone_from(&frame);
                        frame
                    } else if last_active.elapsed() <= Duration::from_millis(300) {
                        last_frame.clone()
                    } else {
                        frame
                    };
                    if out_frame.len() == frame_samples {
                        audio_out.push_frame(&out_frame);
                    } else {
                        break;
                    }
                }
            }
        });
    }

    Ok(VoiceRuntime {
        _audio_in: audio_in,
        mixer,
        frame_samples: audio_config.frame_samples(),
        emit_voice: config.audio.emit_voice_frames,
        audio_config,
        tuning,
    })
}

fn audio_level(frame: &[i16]) -> f32 {
    let mut sum = 0f32;
    for s in frame {
        sum += (*s as f32).abs();
    }
    (sum / frame.len().max(1) as f32) / i16::MAX as f32
}

fn is_frame_active(frame: &[i16]) -> bool {
    let mut sum = 0i64;
    for s in frame {
        sum += (*s as i64).abs();
    }
    let avg = sum / frame.len().max(1) as i64;
    avg > 250
}

fn compute_next_tuning(qos: &mut QosState) -> Option<AudioTuning> {
    if qos.peer_stats.is_empty() {
        return None;
    }
    let now = Instant::now();
    if now.duration_since(qos.last_adjust) < Duration::from_secs(2) {
        return None;
    }
    let mut worst_rtt = 0.0f32;
    let mut worst_loss = 0.0f32;
    for stats in qos.peer_stats.values() {
        worst_rtt = worst_rtt.max(stats.rtt_ms);
        worst_loss = worst_loss.max(stats.loss);
    }

    let mut bitrate = qos.current.bitrate;
    let max_latency = qos.profile.max_latency_ms as f32;
    let target_latency = qos.profile.target_latency_ms as f32;
    if worst_loss > qos.profile.packet_loss_tolerance || worst_rtt > max_latency {
        bitrate = ((bitrate as f32) * 0.8) as u32;
    } else if worst_loss < qos.profile.packet_loss_tolerance * 0.5
        && worst_rtt < target_latency
    {
        bitrate = ((bitrate as f32) * 1.1) as u32;
    }
    bitrate = bitrate
        .clamp(qos.profile.min_bitrate, qos.profile.max_bitrate)
        .max(8_000);

    let fec = worst_loss > qos.profile.packet_loss_tolerance * 0.5;
    let loss_pct = (worst_loss * 100.0).round().min(100.0) as u8;

    let next = AudioTuning {
        bitrate,
        fec,
        loss_pct,
    };
    if next.bitrate != qos.current.bitrate
        || next.fec != qos.current.fec
        || next.loss_pct != qos.current.loss_pct
    {
        rift_metrics::set_gauge("rift_audio_bitrate", &[], next.bitrate as f64);
        tracing::info!(
            bitrate = next.bitrate,
            fec = next.fec,
            loss_pct = next.loss_pct,
            "qos audio tuning updated"
        );
        qos.current = next.clone();
        qos.last_adjust = now;
        return Some(next);
    }
    None
}

fn map_quality_to_bitrate(quality: Option<&str>) -> u32 {
    match quality.unwrap_or("medium") {
        "low" => 24_000,
        "high" => 96_000,
        _ => 48_000,
    }
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn peer_to_stream_id(peer: &PeerId) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&peer.0[..8]);
    u64::from_le_bytes(bytes)
}

fn default_nat_config(
    port: u16,
    ports: Option<Vec<u16>>,
    stun_servers: Vec<String>,
    stun_timeout_ms: Option<u64>,
    punch_interval_ms: Option<u64>,
    punch_timeout_ms: Option<u64>,
) -> NatConfig {
    let mut local_ports = ports.unwrap_or_default();
    if local_ports.is_empty() {
        local_ports.push(port);
        local_ports.push(port.saturating_add(1));
        local_ports.push(port.saturating_add(2));
    }
    NatConfig {
        local_ports,
        stun_servers: parse_socket_addrs(&stun_servers),
        stun_timeout_ms: stun_timeout_ms.unwrap_or(800),
        punch_interval_ms: punch_interval_ms.unwrap_or(200),
        punch_timeout_ms: punch_timeout_ms.unwrap_or(5000),
    }
}

fn derive_auth_token(secret: &str, channel: &str) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(Some(channel.as_bytes()), secret.as_bytes());
    let mut out = [0u8; 32];
    hk.expand(b"rift-auth", &mut out)
        .expect("hkdf expand");
    out.to_vec()
}

fn derive_e2ee_key(
    channel: &str,
    password: Option<&str>,
    invite: Option<&Invite>,
    shared_secret: Option<&str>,
) -> Option<[u8; 32]> {
    if let Some(secret) = shared_secret {
        let hk = Hkdf::<Sha256>::new(Some(channel.as_bytes()), secret.as_bytes());
        let mut out = [0u8; 32];
        hk.expand(b"rift-e2ee", &mut out)
            .expect("hkdf expand");
        return Some(out);
    }
    if let Some(invite) = invite {
        if invite.channel_key.iter().any(|b| *b != 0) {
            return Some(invite.channel_key);
        }
    }
    if let Some(password) = password {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rift-e2ee:");
        hasher.update(channel.as_bytes());
        hasher.update(b":");
        hasher.update(password.as_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        return Some(out);
    }
    None
}

fn short_peer(peer_id: &PeerId) -> String {
    let hex = peer_id.to_hex();
    hex.chars().take(8).collect()
}

fn resolve_known_hosts_path(cfg: &SecurityConfig) -> Result<PathBuf, RiftError> {
    if let Some(path) = &cfg.known_hosts_path {
        return Ok(expand_tilde(path));
    }
    let base = dirs::config_dir().ok_or_else(|| RiftError::Other("config dir missing".to_string()))?;
    Ok(base.join("rift").join("known_hosts"))
}

fn expand_tilde(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(rest) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

fn load_known_hosts(path: &Path) -> HashMap<PeerId, Vec<u8>> {
    let mut map = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(peer_hex) = parts.next() else { continue; };
        let Some(key_hex) = parts.next() else { continue; };
        let Ok(peer_bytes) = hex::decode(peer_hex) else { continue; };
        let Ok(key_bytes) = hex::decode(key_hex) else { continue; };
        if peer_bytes.len() != 32 {
            continue;
        }
        let mut peer = [0u8; 32];
        peer.copy_from_slice(&peer_bytes);
        map.insert(PeerId(peer), key_bytes);
    }
    map
}

fn append_known_host(path: &Path, peer_id: PeerId, public_key: &[u8]) -> Result<(), RiftError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RiftError::Other(format!("{e}")))?;
    }
    let line = format!("{} {}\n", peer_id.to_hex(), hex::encode(public_key));
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()))
        .map_err(|e| RiftError::Other(format!("{e}")))?;
    Ok(())
}

fn fingerprint_key(public_key: &[u8]) -> String {
    let hash = blake3::hash(public_key);
    let hex = hash.to_hex().to_string();
    hex.chars().take(16).collect()
}

async fn handle_peer_identity(
    event_tx: &mpsc::UnboundedSender<RiftEvent>,
    handle: &MeshHandle,
    cfg: &SecurityConfig,
    peer_id: PeerId,
    public_key: &[u8],
) -> Result<(), RiftError> {
    let computed = rift_core::peer_id_from_public_key_bytes(public_key)
        .map_err(|e| RiftError::Other(format!("{e}")))?;
    let fingerprint = fingerprint_key(public_key);
    let known_hosts = resolve_known_hosts_path(cfg)?;
    let mut known = load_known_hosts(&known_hosts);

    if computed != peer_id {
        let msg = format!(
            "peer id mismatch for {} (fingerprint {})",
            short_peer(&peer_id),
            fingerprint
        );
        tracing::warn!(peer = %peer_id, "peer id mismatch");
        let _ = event_tx.send(RiftEvent::SecurityNotice { message: msg.clone() });
        audit_log(cfg, "peer_id_mismatch", &peer_id, Some(&fingerprint), &msg);
        if cfg.reject_on_mismatch {
            handle.disconnect_peer(peer_id).await;
        }
        return Ok(());
    }

    if let Some(existing) = known.get(&peer_id) {
        if existing != public_key {
            let msg = format!(
                "peer key mismatch for {} (fingerprint {})",
                short_peer(&peer_id),
                fingerprint
            );
            tracing::warn!(peer = %peer_id, "peer key mismatch");
            let _ = event_tx.send(RiftEvent::SecurityNotice { message: msg.clone() });
            audit_log(cfg, "peer_key_mismatch", &peer_id, Some(&fingerprint), &msg);
            if cfg.reject_on_mismatch {
                handle.disconnect_peer(peer_id).await;
            }
        }
        return Ok(());
    }

    if cfg.trust_on_first_use {
        append_known_host(&known_hosts, peer_id, public_key)?;
        known.insert(peer_id, public_key.to_vec());
        let msg = format!(
            "new peer: {} fingerprint {} (saved to known_hosts)",
            short_peer(&peer_id),
            fingerprint
        );
        tracing::info!(peer = %peer_id, "new peer key stored");
        let _ = event_tx.send(RiftEvent::SecurityNotice { message: msg.clone() });
        audit_log(cfg, "peer_first_seen", &peer_id, Some(&fingerprint), &msg);
    } else {
        let msg = format!(
            "untrusted peer {} fingerprint {} (TOFU disabled)",
            short_peer(&peer_id),
            fingerprint
        );
        tracing::warn!(peer = %peer_id, "untrusted peer (TOFU disabled)");
        let _ = event_tx.send(RiftEvent::SecurityNotice { message: msg.clone() });
        audit_log(cfg, "peer_untrusted", &peer_id, Some(&fingerprint), &msg);
        handle.disconnect_peer(peer_id).await;
    }
    Ok(())
}

fn audit_log(cfg: &SecurityConfig, event: &str, peer_id: &PeerId, fingerprint: Option<&str>, message: &str) {
    let Some(path) = cfg.audit_log_path.as_ref() else { return; };
    let path = expand_tilde(path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let entry = json!({
        "ts": now_timestamp(),
        "event": event,
        "peer_id": peer_id.to_hex(),
        "fingerprint": fingerprint.unwrap_or(""),
        "message": message,
    });
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = std::io::Write::write_all(&mut file, line.as_bytes());
            let _ = std::io::Write::write_all(&mut file, b"\n");
        }
    }
}

fn parse_socket_addr(input: &str) -> Option<SocketAddr> {
    input.parse::<SocketAddr>().ok()
}

fn parse_socket_addrs(inputs: &[String]) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for input in inputs {
        if let Ok(addr) = input.parse::<SocketAddr>() {
            out.push(addr);
            continue;
        }
        if let Ok(mut iter) = input.to_socket_addrs() {
            if let Some(addr) = iter.next() {
                out.push(addr);
            }
        }
    }
    out
}

#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(target_os = "android")]
pub mod android_jni;
