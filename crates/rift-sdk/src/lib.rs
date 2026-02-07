//! Rift SDK: high-level API for embedding Rift VoIP in other applications.

use std::path::PathBuf;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;

use rift_core::{decode_invite, generate_invite, Identity, Invite, PeerId};
use rift_dht::{DhtConfig as RiftDhtConfig, DhtHandle, PeerEndpointInfo};
use rift_discovery::local_ipv4_addrs;
use rift_media::{
    decode_frame, encode_frame, AudioConfig, AudioIn, AudioMixer, AudioOut, OpusDecoder,
    OpusEncoder,
};
use rift_mesh::{Mesh, MeshConfig, MeshEvent, MeshHandle};
use rift_nat::{attempt_hole_punch, NatConfig, PeerEndpoint};
use rift_protocol::{CallState, Capabilities, SessionId};

pub use rift_core::PeerId as RiftPeerId;
pub use rift_protocol::{
    CallState as RiftCallState, ChatMessage, CodecId, FeatureFlag, SessionId as RiftSessionId,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftConfig {
    pub identity_path: Option<PathBuf>,
    pub listen_port: u16,
    pub relay: bool,
    pub user_name: Option<String>,
    pub preferred_codecs: Vec<CodecId>,
    pub preferred_features: Vec<FeatureFlag>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfigSdk {
    pub prefer_p2p: bool,
    pub local_ports: Option<Vec<u16>>,
    pub known_peers: Vec<std::net::SocketAddr>,
    pub invite: Option<String>,
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
            preferred_features: vec![FeatureFlag::Voice, FeatureFlag::Text, FeatureFlag::Relay],
            dht: DhtConfigSdk::default(),
            audio: AudioConfigSdk::default(),
            network: NetworkConfigSdk::default(),
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
    runtime: Mutex<Option<SessionRuntime>>,
    event_rx: Mutex<mpsc::UnboundedReceiver<RiftEvent>>,
    event_tx: mpsc::UnboundedSender<RiftEvent>,
    ptt_active: Arc<AtomicBool>,
    mute_active: Arc<AtomicBool>,
}

impl RiftHandle {
    pub async fn new(config: RiftConfig) -> Result<Self, RiftError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let identity = Identity::load(config.identity_path.as_deref())
            .context("identity not found")
            .map_err(|e| RiftError::Other(format!("{e}")))?;
        let local_peer_id = identity.peer_id;
        Ok(Self {
            ptt_active: Arc::new(AtomicBool::new(!config.audio.ptt)),
            identity: Mutex::new(Some(identity)),
            local_peer_id,
            config,
            runtime: Mutex::new(None),
            event_rx: Mutex::new(event_rx),
            event_tx,
            mute_active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn join_channel(
        &self,
        name: &str,
        password: Option<&str>,
        internet: bool,
    ) -> Result<(), RiftError> {
        let mut runtime_guard = self.runtime.lock().await;
        if runtime_guard.is_some() {
            return Err(RiftError::AlreadyJoined);
        }
        let identity = {
            let mut identity_guard = self.identity.lock().await;
            match identity_guard.take() {
                Some(identity) => identity,
                None => Identity::load(self.config.identity_path.as_deref())
                    .context("identity not found")
                    .map_err(|e| RiftError::Other(format!("{e}")))?,
            }
        };

        let config = MeshConfig {
            channel_name: name.to_string(),
            password: password.map(|v| v.to_string()),
            listen_port: self.config.listen_port,
            relay_capable: self.config.relay,
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

        if internet {
            let nat_cfg = default_nat_config(self.config.listen_port, self.config.network.local_ports.clone());
            mesh.enable_nat(nat_cfg.clone()).await;
            let invite = if let Some(invite_str) = &self.config.network.invite {
                decode_invite(invite_str).map_err(|e| RiftError::Other(format!("{e}")))?
            } else if !self.config.network.known_peers.is_empty() {
                generate_invite(name, password, self.config.network.known_peers.clone())
            } else {
                Invite {
                    channel_name: name.to_string(),
                    password: password.map(|v| v.to_string()),
                    channel_key: [0u8; 32],
                    known_peers: Vec::new(),
                    version: 1,
                    created_at: now_timestamp(),
                }
            };
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
        let voice = if self.config.audio.enabled {
            Some(start_audio_pipeline(
                self.config.clone(),
                handle.clone(),
                self.ptt_active.clone(),
                self.mute_active.clone(),
            )?)
        } else {
            None
        };

        let dht = if self.config.dht.enabled {
            let dht_config = RiftDhtConfig {
                bootstrap_nodes: parse_socket_addrs(&self.config.dht.bootstrap_nodes),
                listen_addr: self
                    .config
                    .dht
                    .listen_addr
                    .as_deref()
                    .and_then(parse_socket_addr)
                    .unwrap_or_else(|| {
                        SocketAddr::from(([0, 0, 0, 0], self.config.listen_port.saturating_add(100)))
                    }),
            };
            let handle_dht = DhtHandle::new(dht_config)
                .await
                .map_err(|e| RiftError::Other(format!("{e}")))?;

            let channel_id = rift_core::ChannelId::from_channel(&channel, password);
            let addrs = local_ipv4_addrs()
                .map_err(|e| RiftError::Other(format!("{e}")))?
                .into_iter()
                .map(|ip| SocketAddr::new(ip, self.config.listen_port))
                .collect::<Vec<_>>();
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
            let nat_cfg = default_nat_config(self.config.listen_port, self.config.network.local_ports.clone());
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
                    MeshEvent::PeerSessionConfig { .. }
                    | MeshEvent::RouteUpdated { .. }
                    | MeshEvent::RouteUpgraded(_) => {}
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
}

fn start_audio_pipeline(
    config: RiftConfig,
    handle: MeshHandle,
    ptt_active: Arc<AtomicBool>,
    mute_active: Arc<AtomicBool>,
) -> Result<VoiceRuntime, RiftError> {
    let mut audio_config = AudioConfig::default();
    audio_config.bitrate = map_quality_to_bitrate(Some(&config.audio.quality));
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
    tokio::spawn(async move {
        let mut seq: u32 = 0;
        let mut hangover: u8 = 0;
        while let Some(frame) = audio_rx.recv().await {
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

fn default_nat_config(port: u16, ports: Option<Vec<u16>>) -> NatConfig {
    let mut local_ports = ports.unwrap_or_default();
    if local_ports.is_empty() {
        local_ports.push(port);
        local_ports.push(port.saturating_add(1));
        local_ports.push(port.saturating_add(2));
    }
    NatConfig { local_ports }
}

fn parse_socket_addr(input: &str) -> Option<SocketAddr> {
    input.parse::<SocketAddr>().ok()
}

fn parse_socket_addrs(inputs: &[String]) -> Vec<SocketAddr> {
    inputs.iter().filter_map(|s| parse_socket_addr(s)).collect()
}

#[cfg(feature = "ffi")]
pub mod ffi;
