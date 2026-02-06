use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;

use rift_core::message::{
    decode_wire_message, encode_wire_message, ChatMessage, ControlMessage, MessageId, WireMessage,
};
use rift_core::noise::{noise_builder, NoiseSession};
use rift_core::{Identity, PeerId};
use rift_discovery::{discover_peers, start_mdns_advertisement, DiscoveryConfig, MdnsHandle};

const MAX_PACKET: usize = 2048;

#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub channel_name: String,
    pub password: Option<String>,
    pub listen_port: u16,
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
    socket: Arc<UdpSocket>,
    identity: Identity,
    peers_by_id: Mutex<HashMap<PeerId, SocketAddr>>,
    connections: Mutex<HashMap<SocketAddr, PeerConnection>>,
    pending: Mutex<HashMap<SocketAddr, PendingHandshake>>,
    cache: Mutex<HashSet<MessageId>>,
    voice_seq: Mutex<HashMap<PeerId, u32>>,
    events_tx: mpsc::Sender<MeshEvent>,
}

#[derive(Debug)]
struct PeerConnection {
    addr: SocketAddr,
    peer_id: Option<PeerId>,
    session: NoiseSession,
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
            socket: socket.clone(),
            identity,
            peers_by_id: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashSet::new()),
            voice_seq: Mutex::new(HashMap::new()),
            events_tx,
        });

        MeshInner::spawn_receiver(inner.clone());

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
        Ok(self.inner.socket.local_addr()?)
    }

    pub fn start_discovery(&mut self) -> Result<()> {
        let mdns = start_mdns_advertisement(self.discovery_config.clone())?;
        self._mdns = Some(mdns);
        MeshInner::spawn_discovery(self.inner.clone(), self.discovery_config.clone());
        Ok(())
    }

    pub async fn broadcast_chat(&self, text: String) -> Result<()> {
        let timestamp = now_timestamp();
        let chat = ChatMessage::new(self.inner.identity.peer_id, timestamp, text);
        let msg = WireMessage::Control(ControlMessage::Chat(chat.clone()));

        let mut cache = self.inner.cache.lock().await;
        cache.insert(chat.id);
        drop(cache);

        let addrs: Vec<SocketAddr> = {
            let connections = self.inner.connections.lock().await;
            connections.keys().copied().collect()
        };

        for addr in addrs {
            self.inner.send_wire(addr, &msg).await?;
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
    fn spawn_receiver(inner: Arc<Self>) {
        tokio::spawn(async move {
            let mut buf = [0u8; MAX_PACKET];
            loop {
                let (len, addr) = match inner.socket.recv_from(&mut buf).await {
                    Ok(res) => res,
                    Err(_) => continue,
                };
                if let Err(err) = inner.handle_packet(addr, &buf[..len]).await {
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
                if let Err(err) = inner.initiate_handshake(peer.addr).await {
                    eprintln!("handshake to {} failed: {err}", peer.addr);
                }
            }
        });
    }

    async fn initiate_handshake(&self, addr: SocketAddr) -> Result<()> {
        let builder = noise_builder();
        let static_kp = builder.generate_keypair()?;
        let mut hs = builder
            .local_private_key(&static_kp.private)
            .build_initiator()?;

        let mut buf = [0u8; MAX_PACKET];
        let len = hs.write_message(&[], &mut buf)?;
        self.socket.send_to(&buf[..len], addr).await?;

        let mut pending = self.pending.lock().await;
        pending.insert(addr, PendingHandshake::InitiatorAwait2(hs));
        Ok(())
    }

    async fn handle_packet(&self, addr: SocketAddr, data: &[u8]) -> Result<()> {
        if self.try_handle_pending(addr, data).await? {
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
                self.start_responder(addr, data).await?;
                return Ok(());
            }
        };

        let msg = decode_wire_message(&plaintext)?;
        self.handle_wire(addr, msg).await?;
        Ok(())
    }

    async fn try_handle_pending(&self, addr: SocketAddr, data: &[u8]) -> Result<bool> {
        let mut pending = self.pending.lock().await;
        let Some(state) = pending.remove(&addr) else {
            return Ok(false);
        };

        match state {
            PendingHandshake::InitiatorAwait2(mut hs) => {
                let mut out = [0u8; MAX_PACKET];
                hs.read_message(data, &mut out)?;
                let len = hs.write_message(&[], &mut out)?;
                self.socket.send_to(&out[..len], addr).await?;
                let transport = hs.into_transport_mode()?;
                self.install_connection(addr, transport).await?;
            }
            PendingHandshake::ResponderAwait3(mut hs) => {
                let mut out = [0u8; MAX_PACKET];
                hs.read_message(data, &mut out)?;
                let transport = hs.into_transport_mode()?;
                self.install_connection(addr, transport).await?;
            }
        }

        Ok(true)
    }

    async fn start_responder(&self, addr: SocketAddr, first_msg: &[u8]) -> Result<()> {
        let builder = noise_builder();
        let static_kp = builder.generate_keypair()?;
        let mut hs = builder
            .local_private_key(&static_kp.private)
            .build_responder()?;

        let mut out = [0u8; MAX_PACKET];
        hs.read_message(first_msg, &mut out)?;
        let len = hs.write_message(&[], &mut out)?;
        self.socket.send_to(&out[..len], addr).await?;

        let mut pending = self.pending.lock().await;
        pending.insert(addr, PendingHandshake::ResponderAwait3(hs));
        Ok(())
    }

    async fn install_connection(
        &self,
        addr: SocketAddr,
        transport: snow::TransportState,
    ) -> Result<()> {
        let conn = PeerConnection {
            addr,
            peer_id: None,
            session: NoiseSession::new(transport),
        };
        let mut connections = self.connections.lock().await;
        connections.insert(addr, conn);
        drop(connections);

        let join = WireMessage::Control(ControlMessage::Join {
            peer_id: self.identity.peer_id,
        });
        self.send_wire(addr, &join).await?;
        Ok(())
    }

    async fn send_wire(&self, addr: SocketAddr, msg: &WireMessage) -> Result<()> {
        let plaintext = encode_wire_message(msg);
        let ciphertext = {
            let mut connections = self.connections.lock().await;
            let Some(conn) = connections.get_mut(&addr) else {
                return Err(anyhow!("missing connection"));
            };
            let mut out = vec![0u8; plaintext.len() + 128];
            let len = conn.session.encrypt(&plaintext, &mut out)?;
            out.truncate(len);
            out
        };
        self.socket.send_to(&ciphertext, addr).await?;
        Ok(())
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
        let addrs: Vec<SocketAddr> = {
            let connections = self.connections.lock().await;
            connections.keys().copied().collect()
        };
        for addr in addrs {
            self.send_wire(addr, &msg).await?;
        }
        Ok(())
    }

    async fn handle_wire(&self, addr: SocketAddr, msg: WireMessage) -> Result<()> {
        match msg {
            WireMessage::Control(ControlMessage::Join { peer_id }) => {
                let mut connections = self.connections.lock().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    conn.peer_id = Some(peer_id);
                }
                drop(connections);

                let mut peers = self.peers_by_id.lock().await;
                peers.insert(peer_id, addr);
                drop(peers);

                let _ = self.events_tx.send(MeshEvent::PeerJoined(peer_id)).await;
            }
            WireMessage::Control(ControlMessage::Leave { peer_id }) => {
                let mut peers = self.peers_by_id.lock().await;
                peers.remove(&peer_id);
                drop(peers);
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

                self.gossip(addr, WireMessage::Control(ControlMessage::Chat(chat)))
                    .await?;
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

                self.gossip(
                    addr,
                    WireMessage::Voice {
                        from,
                        seq,
                        timestamp,
                        payload,
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn gossip(&self, from: SocketAddr, msg: WireMessage) -> Result<()> {
        let addrs: Vec<SocketAddr> = {
            let connections = self.connections.lock().await;
            connections
                .keys()
                .filter(|addr| **addr != from)
                .copied()
                .collect()
        };
        for addr in addrs {
            if let Err(err) = self.send_wire(addr, &msg).await {
                eprintln!("gossip send to {} failed: {err}", addr);
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
