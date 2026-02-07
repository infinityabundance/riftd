use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;

use rift_core::message::{
    decode_wire_message, encode_wire_message, ChatMessage, ControlMessage, MessageId, PeerInfo,
    WireMessage,
};
use rift_core::{Identity, Invite, PeerId};
use rift_discovery::{discover_peers, start_mdns_advertisement, DiscoveryConfig, MdnsHandle};
use rift_nat::{attempt_hole_punch, NatConfig, PeerEndpoint};

const MAX_PACKET: usize = 2048;

#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub channel_name: String,
    pub password: Option<String>,
    pub listen_port: u16,
    pub relay_capable: bool,
}

#[derive(Debug, Clone)]
pub enum PeerRoute {
    Direct { addr: SocketAddr },
    Relayed { via: PeerId },
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    PeerJoined(PeerId),
    PeerLeft(PeerId),
    ChatReceived(ChatMessage),
    VoiceFrame {
        from: PeerId,
        seq: u32,
        timestamp: u64,
        payload: Vec<u8>,
    },
    RouteUpdated { peer_id: PeerId, route: PeerRoute },
    RouteUpgraded(PeerId),
}

pub struct Mesh {
    inner: Arc<MeshInner>,
    events_rx: mpsc::Receiver<MeshEvent>,
    discovery_config: DiscoveryConfig,
    _mdns: Option<MdnsHandle>,
}

#[derive(Clone)]
pub struct MeshHandle {
    inner: Arc<MeshInner>,
}

struct MeshInner {
    sockets: Mutex<Vec<Arc<UdpSocket>>>,
    identity: Identity,
    peers_by_id: Mutex<HashMap<PeerId, SocketAddr>>,
    peer_caps: Mutex<HashMap<PeerId, bool>>,
    routes: Mutex<HashMap<PeerId, PeerRoute>>,
    peer_addrs: Mutex<HashMap<PeerId, SocketAddr>>,
    connections: Mutex<HashMap<SocketAddr, PeerConnection>>,
    pending: Mutex<HashMap<PendingKey, PendingHandshake>>,
    cache: Mutex<HashSet<MessageId>>,
    voice_seq: Mutex<HashMap<PeerId, u32>>,
    nat_cfg: Mutex<Option<NatConfig>>,
    events_tx: mpsc::Sender<MeshEvent>,
    relay_capable: bool,
}

#[derive(Debug)]
struct PeerConnection {
    addr: SocketAddr,
    peer_id: Option<PeerId>,
    session: rift_core::noise::NoiseSession,
    socket_idx: usize,
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

impl Mesh {
    pub async fn new(identity: Identity, config: MeshConfig) -> Result<Self> {
        let addr = SocketAddr::from(([0, 0, 0, 0], config.listen_port));
        let socket = UdpSocket::bind(addr).await?;
        let socket = Arc::new(socket);

        let (events_tx, events_rx) = mpsc::channel(256);
        let inner = Arc::new(MeshInner {
            sockets: Mutex::new(vec![socket.clone()]),
            identity,
            peers_by_id: Mutex::new(HashMap::new()),
            peer_caps: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
            peer_addrs: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashSet::new()),
            voice_seq: Mutex::new(HashMap::new()),
            nat_cfg: Mutex::new(None),
            events_tx,
            relay_capable: config.relay_capable,
        });

        MeshInner::spawn_receiver(inner.clone(), 0, socket.clone());
        MeshInner::spawn_auto_upgrade(inner.clone());

        let mesh = Self {
            inner,
            events_rx,
            discovery_config: DiscoveryConfig {
                channel_name: config.channel_name,
                password: config.password,
                peer_id: identity.peer_id,
                listen_port: socket.local_addr()?.port(),
            },
            _mdns: None,
        };

        Ok(mesh)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        let sockets = self.inner.sockets.blocking_lock();
        Ok(sockets[0].local_addr()?)
    }

    pub fn start_lan_discovery(&mut self) -> Result<()> {
        let mdns = start_mdns_advertisement(self.discovery_config.clone())?;
        self._mdns = Some(mdns);
        MeshInner::spawn_discovery(self.inner.clone(), self.discovery_config.clone());
        Ok(())
    }

    pub async fn enable_nat(&mut self, nat_cfg: NatConfig) {
        let mut cfg = self.inner.nat_cfg.lock().await;
        *cfg = Some(nat_cfg);
    }

    pub async fn join_invite(&mut self, invite: Invite, nat_cfg: NatConfig) -> Result<()> {
        {
            let mut cfg = self.inner.nat_cfg.lock().await;
            *cfg = Some(nat_cfg.clone());
        }

        for addr in invite.known_peers {
            let endpoint = PeerEndpoint {
                peer_id: PeerId([0u8; 32]),
                external_addrs: vec![addr],
                punch_ports: vec![addr.port()],
            };
            if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                let socket_idx = self.inner.add_socket(socket).await?;
                if let Err(err) = self.inner.initiate_handshake(remote, socket_idx).await {
                    eprintln!("handshake to {} failed: {err}", remote);
                }
            }
        }
        Ok(())
    }

    pub async fn broadcast_chat(&self, text: String) -> Result<()> {
        let timestamp = now_timestamp();
        let chat = ChatMessage::new(self.inner.identity.peer_id, timestamp, text);
        let msg = WireMessage::Control(ControlMessage::Chat(chat.clone()));

        let mut cache = self.inner.cache.lock().await;
        cache.insert(chat.id);
        drop(cache);

        let routes = self.inner.routes_snapshot().await;
        for (peer_id, route) in routes {
            if peer_id == self.inner.identity.peer_id {
                continue;
            }
            if let Err(err) = self.inner.send_to_peer(peer_id, route, msg.clone()).await {
                tracing::debug!(peer = %peer_id, "chat send failed: {err}");
            }
        }

        Ok(())
    }

    pub async fn broadcast_voice(&self, seq: u32, timestamp: u64, payload: Vec<u8>) -> Result<()> {
        self.inner
            .broadcast_voice(self.inner.identity.peer_id, seq, timestamp, payload)
            .await
    }

    pub async fn next_event(&mut self) -> Option<MeshEvent> {
        self.events_rx.recv().await
    }

    pub fn handle(&self) -> MeshHandle {
        MeshHandle {
            inner: self.inner.clone(),
        }
    }
}

impl MeshInner {
    fn spawn_receiver(inner: Arc<Self>, socket_idx: usize, socket: Arc<UdpSocket>) {
        tokio::spawn(async move {
            let mut buf = [0u8; MAX_PACKET];
            loop {
                let (len, addr) = match socket.recv_from(&mut buf).await {
                    Ok(res) => res,
                    Err(_) => continue,
                };
                if let Err(err) = inner
                    .clone()
                    .handle_packet(socket_idx, addr, &buf[..len])
                    .await
                {
                    eprintln!("mesh recv error: {err}");
                }
            }
        });
    }

    fn spawn_discovery(inner: Arc<Self>, config: DiscoveryConfig) {
        tokio::spawn(async move {
            let stream = match discover_peers(config) {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("discovery failed: {err}");
                    return;
                }
            };
            tokio::pin!(stream);
            while let Some(peer) = stream.next().await {
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
                    eprintln!("handshake to {} failed: {err}", peer.addr);
                }
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
                let relayed: Vec<(PeerId, SocketAddr)> = {
                    let routes = inner.routes.lock().await;
                    let addrs = inner.peer_addrs.lock().await;
                    routes
                        .iter()
                        .filter_map(|(peer_id, route)| match route {
                            PeerRoute::Relayed { .. } => addrs
                                .get(peer_id)
                                .copied()
                                .map(|addr| (*peer_id, addr)),
                            _ => None,
                        })
                        .collect()
                };

                for (peer_id, addr) in relayed {
                    let endpoint = PeerEndpoint {
                        peer_id,
                        external_addrs: vec![addr],
                        punch_ports: vec![addr.port()],
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

    async fn add_socket(self: &Arc<Self>, socket: UdpSocket) -> Result<usize> {
        let socket = Arc::new(socket);
        let mut sockets = self.sockets.lock().await;
        sockets.push(socket.clone());
        let idx = sockets.len() - 1;
        drop(sockets);
        Self::spawn_receiver(self.clone(), idx, socket);
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
        let socket = self.socket_by_idx(socket_idx).await?;
        socket.send_to(&buf[..len], addr).await?;

        let mut pending = self.pending.lock().await;
        pending.insert(
            PendingKey { socket_idx, addr },
            PendingHandshake::InitiatorAwait2(hs),
        );
        Ok(())
    }

    async fn handle_packet(self: Arc<Self>, socket_idx: usize, addr: SocketAddr, data: &[u8]) -> Result<()> {
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

        let msg = decode_wire_message(&plaintext)?;
        self.handle_wire(addr, msg).await?;
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

        match state {
            PendingHandshake::InitiatorAwait2(mut hs) => {
                let mut out = [0u8; MAX_PACKET];
                hs.read_message(data, &mut out)?;
                let len = hs.write_message(&[], &mut out)?;
                let socket = self.socket_by_idx(socket_idx).await?;
                socket.send_to(&out[..len], addr).await?;
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
        let socket = self.socket_by_idx(socket_idx).await?;
        socket.send_to(&out[..len], addr).await?;

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
            addr,
            peer_id: None,
            session: rift_core::noise::NoiseSession::new(transport),
            socket_idx,
        };
        let mut connections = self.connections.lock().await;
        connections.insert(addr, conn);
        drop(connections);

        let join = WireMessage::Control(ControlMessage::Join {
            peer_id: self.identity.peer_id,
            relay_capable: self.relay_capable,
        });
        self.send_wire(addr, &join).await?;

        self.send_peer_list(addr).await?;
        Ok(())
    }

    async fn send_peer_list(&self, addr: SocketAddr) -> Result<()> {
        let peers: Vec<PeerInfo> = {
            let peers_by_id = self.peers_by_id.lock().await;
            let peer_caps = self.peer_caps.lock().await;
            peers_by_id
                .iter()
                .map(|(peer_id, addr)| PeerInfo {
                    peer_id: *peer_id,
                    addr: *addr,
                    relay_capable: *peer_caps.get(peer_id).unwrap_or(&false),
                })
                .collect()
        };
        let msg = WireMessage::Control(ControlMessage::PeerList { peers });
        self.send_wire(addr, &msg).await
    }

    async fn send_wire(&self, addr: SocketAddr, msg: &WireMessage) -> Result<()> {
        let plaintext = encode_wire_message(msg);
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
        let socket = self.socket_by_idx(socket_idx).await?;
        socket.send_to(&ciphertext, addr).await?;
        Ok(())
    }

    async fn handle_wire(self: Arc<Self>, addr: SocketAddr, msg: WireMessage) -> Result<()> {
        match msg {
            WireMessage::Control(ControlMessage::Join {
                peer_id,
                relay_capable,
            }) => {
                let mut connections = self.connections.lock().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    conn.peer_id = Some(peer_id);
                }
                drop(connections);

                let mut peers = self.peers_by_id.lock().await;
                peers.insert(peer_id, addr);
                drop(peers);

                let mut peer_caps = self.peer_caps.lock().await;
                peer_caps.insert(peer_id, relay_capable);
                drop(peer_caps);

                let mut peer_addrs = self.peer_addrs.lock().await;
                peer_addrs.insert(peer_id, addr);
                drop(peer_addrs);

                let mut routes = self.routes.lock().await;
                let upgraded = matches!(routes.get(&peer_id), Some(PeerRoute::Relayed { .. }));
                routes.insert(peer_id, PeerRoute::Direct { addr });
                drop(routes);

                let _ = self.events_tx.send(MeshEvent::PeerJoined(peer_id)).await;
                let _ = self
                    .events_tx
                    .send(MeshEvent::RouteUpdated {
                        peer_id,
                        route: PeerRoute::Direct { addr },
                    })
                    .await;
                if upgraded {
                    let _ = self.events_tx.send(MeshEvent::RouteUpgraded(peer_id)).await;
                    tracing::info!(peer = %peer_id, "route upgraded to direct");
                }

                self.send_peer_list(addr).await?;
            }
            WireMessage::Control(ControlMessage::Leave { peer_id }) => {
                let mut peers = self.peers_by_id.lock().await;
                peers.remove(&peer_id);
                drop(peers);
                let mut peer_caps = self.peer_caps.lock().await;
                peer_caps.remove(&peer_id);
                drop(peer_caps);
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
                let _ = self.events_tx.send(MeshEvent::PeerLeft(peer_id)).await;
            }
            WireMessage::Control(ControlMessage::Chat(chat)) => {
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
            WireMessage::Control(ControlMessage::PeerList { peers }) => {
                self.handle_peer_list(addr, peers).await?;
            }
            WireMessage::Voice {
                from,
                seq,
                timestamp,
                payload,
            } => {
                let mut seqs = self.voice_seq.lock().await;
                if let Some(last) = seqs.get(&from) {
                    if seq <= *last {
                        return Ok(());
                    }
                }
                seqs.insert(from, seq);
                drop(seqs);

                let _ = self
                    .events_tx
                    .send(MeshEvent::VoiceFrame {
                        from,
                        seq,
                        timestamp,
                        payload: payload.clone(),
                    })
                    .await;
            }
            WireMessage::Relay { target, inner } => {
                if target == self.identity.peer_id {
                    self.handle_wire(addr, *inner).await?;
                } else {
                    self.forward_relay(target, *inner).await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_peer_list(self: Arc<Self>, addr: SocketAddr, peers: Vec<PeerInfo>) -> Result<()> {
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
            let mut peer_addrs = self.peer_addrs.lock().await;
            peer_addrs.insert(peer.peer_id, peer.addr);
            drop(peer_addrs);

            if already {
                continue;
            }
            let already = {
                let connections = self.connections.lock().await;
                connections.contains_key(&peer.addr)
            };
            if already {
                continue;
            }
            let endpoint = PeerEndpoint {
                peer_id: PeerId([0u8; 32]),
                external_addrs: vec![peer.addr],
                punch_ports: vec![peer.addr.port()],
            };
            if let Ok((socket, remote)) = attempt_hole_punch(&nat_cfg, &endpoint).await {
                if let Ok(socket_idx) = self.add_socket(socket).await {
                    if let Err(err) = self.initiate_handshake(remote, socket_idx).await {
                        eprintln!("handshake to {} failed: {err}", remote);
                    }
                }
            } else if relay_capable {
                if let Some(relay_peer_id) = relay_peer_id {
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

    async fn socket_by_idx(&self, socket_idx: usize) -> Result<Arc<UdpSocket>> {
        let sockets = self.sockets.lock().await;
        sockets
            .get(socket_idx)
            .cloned()
            .ok_or_else(|| anyhow!("missing socket"))
    }

    async fn broadcast_voice(
        &self,
        from: PeerId,
        seq: u32,
        timestamp: u64,
        payload: Vec<u8>,
    ) -> Result<()> {
        let msg = WireMessage::Voice {
            from,
            seq,
            timestamp,
            payload,
        };
        let routes = self.routes_snapshot().await;
        for (peer_id, route) in routes {
            if peer_id == self.identity.peer_id {
                continue;
            }
            if let Err(err) = self.send_to_peer(peer_id, route, msg.clone()).await {
                tracing::debug!(peer = %peer_id, "voice send failed: {err}");
            }
        }
        Ok(())
    }

    async fn routes_snapshot(&self) -> HashMap<PeerId, PeerRoute> {
        self.routes.lock().await.clone()
    }

    async fn send_to_peer(&self, peer_id: PeerId, route: PeerRoute, msg: WireMessage) -> Result<()> {
        match route {
            PeerRoute::Direct { addr } => self.send_wire(addr, &msg).await,
            PeerRoute::Relayed { via } => {
                let relay_addr = {
                    let peers = self.peers_by_id.lock().await;
                    peers.get(&via).copied()
                };
                let Some(relay_addr) = relay_addr else {
                    return Err(anyhow!("missing relay addr"));
                };
                let envelope = WireMessage::Relay {
                    target: peer_id,
                    inner: Box::new(msg),
                };
                self.send_wire(relay_addr, &envelope).await
            }
        }
    }

    async fn forward_relay(&self, target: PeerId, inner: WireMessage) -> Result<()> {
        let route = {
            let routes = self.routes.lock().await;
            routes.get(&target).cloned()
        };
        let Some(route) = route else {
            return Ok(());
        };
        match route {
            PeerRoute::Direct { addr } => {
                let envelope = WireMessage::Relay {
                    target,
                    inner: Box::new(inner),
                };
                self.send_wire(addr, &envelope).await?;
            }
            PeerRoute::Relayed { via } => {
                let relay_addr = {
                    let peers = self.peers_by_id.lock().await;
                    peers.get(&via).copied()
                };
                if let Some(relay_addr) = relay_addr {
                    let envelope = WireMessage::Relay {
                        target,
                        inner: Box::new(inner),
                    };
                    self.send_wire(relay_addr, &envelope).await?;
                }
            }
        }
        Ok(())
    }
}

impl MeshHandle {
    pub async fn broadcast_voice(&self, seq: u32, timestamp: u64, payload: Vec<u8>) -> Result<()> {
        self.inner
            .broadcast_voice(self.inner.identity.peer_id, seq, timestamp, payload)
            .await
    }
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
