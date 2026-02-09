//! TURN (Traversal Using Relays around NAT) client implementation.
//!
//! This module provides a minimal TURN allocator for UDP relays. It supports:
//! - Allocation requests with long-term credentials
//! - Permission and channel binding management
//! - Send/receive data indications
//!
//! The implementation favors simplicity and correctness over completeness.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::timeout;

use rand::RngCore;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use crc32fast::Hasher as Crc32;

use crate::{StunError};

type HmacSha1 = Hmac<Sha1>;

const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
const STUN_HEADER_LEN: usize = 20;

const TURN_ALLOCATE_REQUEST: u16 = 0x0003;
const TURN_ALLOCATE_RESPONSE: u16 = 0x0103;
const TURN_ALLOCATE_ERROR: u16 = 0x0113;
const TURN_CREATE_PERMISSION_REQUEST: u16 = 0x0008;
const TURN_CREATE_PERMISSION_RESPONSE: u16 = 0x0108;
const TURN_CHANNEL_BIND_REQUEST: u16 = 0x0009;
const TURN_CHANNEL_BIND_RESPONSE: u16 = 0x0109;
const TURN_SEND_INDICATION: u16 = 0x0016;
const TURN_DATA_INDICATION: u16 = 0x0017;

const ATTR_USERNAME: u16 = 0x0006;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const ATTR_ERROR_CODE: u16 = 0x0009;
const ATTR_REALM: u16 = 0x0014;
const ATTR_NONCE: u16 = 0x0015;
const ATTR_XOR_RELAYED_ADDRESS: u16 = 0x0016;
const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
const ATTR_XOR_PEER_ADDRESS: u16 = 0x0012;
const ATTR_DATA: u16 = 0x0013;
const ATTR_CHANNEL_NUMBER: u16 = 0x000C;
const ATTR_LIFETIME: u16 = 0x000D;
const ATTR_FINGERPRINT: u16 = 0x8028;

const TURN_UDP_TRANSPORT: u8 = 17;
const TURN_DEFAULT_PORT: u16 = 3478;

#[derive(Debug, Clone)]
pub struct TurnServerConfig {
    /// TURN server socket address.
    pub addr: SocketAddr,
    /// Optional long-term auth username.
    pub username: Option<String>,
    /// Optional long-term auth credential (password).
    pub credential: Option<String>,
}

#[derive(Debug)]
pub struct TurnRelay {
    /// UDP socket used for TURN control/data.
    socket: Arc<UdpSocket>,
    /// TURN server address.
    server: SocketAddr,
    /// Allocated relay address provided by the TURN server.
    relay_addr: SocketAddr,
    /// TURN realm provided by server during auth challenge.
    realm: Option<String>,
    /// TURN nonce provided by server during auth challenge.
    nonce: Option<String>,
    /// Username for long-term auth.
    username: Option<String>,
    /// Credential for long-term auth.
    credential: Option<String>,
    /// Cached channel bindings (peer addr -> channel number).
    channels: Mutex<HashMap<SocketAddr, u16>>,
    /// Cached permissions for peer addresses.
    permissions: Mutex<HashSet<SocketAddr>>,
}

#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// No TURN servers configured.
    #[error("no turn servers configured")]
    NoServers,
    /// TURN credentials were required but missing.
    #[error("turn server missing credentials")]
    MissingCredentials,
    /// Allocation attempt failed after retries.
    #[error("turn allocation failed")]
    AllocationFailed,
    /// Response from TURN server was invalid.
    #[error("turn response invalid")]
    InvalidResponse,
    /// Authentication failed.
    #[error("turn auth failed")]
    AuthFailed,
    /// Underlying socket I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// STUN parsing errors.
    #[error("stun error: {0}")]
    Stun(#[from] StunError),
}

#[derive(Debug, Clone)]
pub struct TurnCandidate {
    /// Relay address on the TURN server.
    pub relay_addr: SocketAddr,
    /// TURN server address.
    pub server: SocketAddr,
    /// Shared relay handle.
    pub relay: Arc<TurnRelay>,
}

/// Periodically send empty datagrams to keep the TURN allocation alive.
pub fn spawn_turn_keepalive(relay: Arc<TurnRelay>, interval_ms: u64) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(interval_ms.max(1000)));
        loop {
            tick.tick().await;
            let _ = relay.send_to(relay.relay_addr(), b"").await;
        }
    })
}

/// Parse a TURN server URI into a config struct.
pub fn parse_turn_server(uri: &str) -> Result<TurnServerConfig, TurnError> {
    let trimmed = uri.trim();
    let trimmed = trimmed.strip_prefix("turn:").unwrap_or(trimmed);
    let (host_port, query) = match trimmed.split_once('?') {
        Some((base, q)) => (base, Some(q)),
        None => (trimmed, None),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(TURN_DEFAULT_PORT)),
        None => (host_port, TURN_DEFAULT_PORT),
    };
    let addr = format!("{}:{}", host, port)
        .parse::<SocketAddr>()
        .map_err(|_| TurnError::InvalidResponse)?;
    let mut username = None;
    let mut credential = None;
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                if k == "username" {
                    username = Some(v.to_string());
                } else if k == "credential" || k == "password" {
                    credential = Some(v.to_string());
                }
            }
        }
    }
    Ok(TurnServerConfig {
        addr,
        username,
        credential,
    })
}

pub async fn allocate_turn_relay(
    server: TurnServerConfig,
    timeout_ms: u64,
) -> Result<TurnCandidate, TurnError> {
    // Allocate a TURN relay:
    // 1) Send allocate request
    // 2) Handle auth challenge (nonce/realm)
    // 3) Extract relayed address on success
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    let socket = Arc::new(socket);

    let mut nonce = None;
    let mut realm = None;
    let mut relay_addr = None;

    for attempt in 0..=1 {
        let tx_id = random_tx_id();
        let mut msg = build_allocate_request(&tx_id, server.username.as_deref(), &nonce, &realm)?;
        if let (Some(username), Some(password), Some(realm), Some(_nonce)) = (
            server.username.as_deref(),
            server.credential.as_deref(),
            realm.as_deref(),
            nonce.as_deref(),
        ) {
            add_message_integrity(&mut msg, username, realm, password);
            add_fingerprint(&mut msg);
        }

        socket.send_to(&msg, server.addr).await?;

        let mut buf = [0u8; 1500];
        let (len, _) = timeout(Duration::from_millis(timeout_ms), socket.recv_from(&mut buf))
            .await
            .map_err(|_| TurnError::AllocationFailed)??;
        let response = parse_turn_response(&buf[..len], &tx_id)?;

        match response.kind {
            TurnResponseKind::Success { relayed } => {
                relay_addr = Some(relayed);
                break;
            }
            TurnResponseKind::AuthChallenge { nonce: new_nonce, realm: new_realm } => {
                if attempt == 0 {
                    nonce = Some(new_nonce);
                    realm = Some(new_realm);
                    continue;
                }
                return Err(TurnError::AuthFailed);
            }
            TurnResponseKind::Error => return Err(TurnError::AllocationFailed),
        }
    }

    let relay_addr = relay_addr.ok_or(TurnError::AllocationFailed)?;
    let relay = Arc::new(TurnRelay {
        socket: socket.clone(),
        server: server.addr,
        relay_addr,
        realm,
        nonce,
        username: server.username.clone(),
        credential: server.credential.clone(),
        channels: Mutex::new(HashMap::new()),
        permissions: Mutex::new(HashSet::new()),
    });

    Ok(TurnCandidate {
        relay_addr,
        server: server.addr,
        relay,
    })
}

impl TurnRelay {
    /// Return the allocated relay address.
    pub fn relay_addr(&self) -> SocketAddr {
        self.relay_addr
    }

    /// Send a payload to a peer via TURN (channel data preferred, send indication fallback).
    pub async fn send_to(&self, peer: SocketAddr, data: &[u8]) -> Result<(), TurnError> {
        self.ensure_permission(peer).await?;
        let channel = self.ensure_channel(peer).await.ok();
        if let Some(channel) = channel {
            let mut buf = Vec::with_capacity(4 + data.len());
            buf.extend_from_slice(&channel.to_be_bytes());
            buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
            buf.extend_from_slice(data);
            self.socket.send_to(&buf, self.server).await?;
            return Ok(());
        }

        let tx_id = random_tx_id();
        let mut msg = build_send_indication(&tx_id, peer, data);
        if self.should_auth() {
            self.add_auth(&mut msg)?;
        }
        self.socket.send_to(&msg, self.server).await?;
        Ok(())
    }

    /// Receive the next payload from the TURN relay.
    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), TurnError> {
        loop {
            let (len, _addr) = self.socket.recv_from(buf).await?;
            if len >= 4 && is_channel_data(buf) {
                let channel = u16::from_be_bytes([buf[0], buf[1]]);
                let data_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
                if len < 4 + data_len {
                    continue;
                }
                let peer = {
                    let channels = self.channels.lock().await;
                    channels.iter().find(|(_, c)| **c == channel).map(|(peer, _)| *peer)
                };
                if let Some(peer) = peer {
                    buf.copy_within(4..4 + data_len, 0);
                    return Ok((data_len, peer));
                }
            }

            if let Ok((peer, payload)) = parse_data_indication(&buf[..len]) {
                let payload_len = payload.len();
                if payload_len > buf.len() {
                    continue;
                }
                let mut tmp = vec![0u8; payload_len];
                tmp.copy_from_slice(payload);
                buf[..payload_len].copy_from_slice(&tmp);
                return Ok((payload_len, peer));
            }
        }
    }

    async fn ensure_permission(&self, peer: SocketAddr) -> Result<(), TurnError> {
        // TURN requires explicit permission before relaying to a peer.
        {
            let perms = self.permissions.lock().await;
            if perms.contains(&peer) {
                return Ok(());
            }
        }
        let tx_id = random_tx_id();
        let mut msg = build_create_permission(&tx_id, peer);
        if self.should_auth() {
            self.add_auth(&mut msg)?;
        }
        self.socket.send_to(&msg, self.server).await?;
        let mut perms = self.permissions.lock().await;
        perms.insert(peer);
        Ok(())
    }

    async fn ensure_channel(&self, peer: SocketAddr) -> Result<u16, TurnError> {
        // Channel bindings provide efficient data framing for frequent peers.
        if let Some(channel) = { self.channels.lock().await.get(&peer).copied() } {
            return Ok(channel);
        }
        let channel = allocate_channel_number(&self.channels).await;
        let tx_id = random_tx_id();
        let mut msg = build_channel_bind(&tx_id, peer, channel);
        if self.should_auth() {
            self.add_auth(&mut msg)?;
        }
        self.socket.send_to(&msg, self.server).await?;
        let mut channels = self.channels.lock().await;
        channels.insert(peer, channel);
        Ok(channel)
    }

    fn should_auth(&self) -> bool {
        // Auth is required once we have all parameters for long-term credentials.
        self.username.is_some() && self.credential.is_some() && self.realm.is_some() && self.nonce.is_some()
    }

    fn add_auth(&self, msg: &mut Vec<u8>) -> Result<(), TurnError> {
        // Add MESSAGE-INTEGRITY and FINGERPRINT to a TURN message.
        let username = self.username.as_ref().ok_or(TurnError::MissingCredentials)?;
        let credential = self.credential.as_ref().ok_or(TurnError::MissingCredentials)?;
        let realm = self.realm.as_ref().ok_or(TurnError::MissingCredentials)?;
        let _nonce = self.nonce.as_ref().ok_or(TurnError::MissingCredentials)?;
        add_message_integrity(msg, username, realm, credential);
        add_fingerprint(msg);
        Ok(())
    }
}

#[derive(Debug)]
struct TurnResponse {
    kind: TurnResponseKind,
}

#[derive(Debug)]
enum TurnResponseKind {
    Success { relayed: SocketAddr },
    AuthChallenge { nonce: String, realm: String },
    Error,
}

fn build_allocate_request(
    tx_id: &[u8; 12],
    username: Option<&str>,
    nonce: &Option<String>,
    realm: &Option<String>,
) -> Result<Vec<u8>, TurnError> {
    // Build TURN allocate request with optional auth parameters.
    let mut msg = build_stun_header(TURN_ALLOCATE_REQUEST, tx_id);
    add_attr_u32(&mut msg, ATTR_REQUESTED_TRANSPORT, (TURN_UDP_TRANSPORT as u32) << 24);
    if let Some(username) = username {
        add_attr_bytes(&mut msg, ATTR_USERNAME, username.as_bytes());
    }
    if let Some(realm) = realm.as_ref() {
        add_attr_bytes(&mut msg, ATTR_REALM, realm.as_bytes());
    }
    if let Some(nonce) = nonce.as_ref() {
        add_attr_bytes(&mut msg, ATTR_NONCE, nonce.as_bytes());
    }
    finalize_length(&mut msg);
    Ok(msg)
}

fn build_create_permission(tx_id: &[u8; 12], peer: SocketAddr) -> Vec<u8> {
    // Build TURN create-permission request for a peer.
    let mut msg = build_stun_header(TURN_CREATE_PERMISSION_REQUEST, tx_id);
    add_attr_bytes(&mut msg, ATTR_XOR_PEER_ADDRESS, &encode_xor_addr(peer, tx_id));
    finalize_length(&mut msg);
    msg
}

fn build_channel_bind(tx_id: &[u8; 12], peer: SocketAddr, channel: u16) -> Vec<u8> {
    // Build TURN channel bind request for a peer + channel.
    let mut msg = build_stun_header(TURN_CHANNEL_BIND_REQUEST, tx_id);
    add_attr_u32(&mut msg, ATTR_CHANNEL_NUMBER, (channel as u32) << 16);
    add_attr_bytes(&mut msg, ATTR_XOR_PEER_ADDRESS, &encode_xor_addr(peer, tx_id));
    finalize_length(&mut msg);
    msg
}

fn build_send_indication(tx_id: &[u8; 12], peer: SocketAddr, data: &[u8]) -> Vec<u8> {
    // Build TURN send indication (no channel binding required).
    let mut msg = build_stun_header(TURN_SEND_INDICATION, tx_id);
    add_attr_bytes(&mut msg, ATTR_XOR_PEER_ADDRESS, &encode_xor_addr(peer, tx_id));
    add_attr_bytes(&mut msg, ATTR_DATA, data);
    finalize_length(&mut msg);
    msg
}

fn parse_turn_response(buf: &[u8], tx_id: &[u8; 12]) -> Result<TurnResponse, TurnError> {
    // Parse STUN/TURN response and extract success or auth challenge.
    if buf.len() < STUN_HEADER_LEN {
        return Err(TurnError::InvalidResponse);
    }
    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    let msg_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if cookie != STUN_MAGIC_COOKIE || &buf[8..20] != tx_id {
        return Err(TurnError::InvalidResponse);
    }
    let end = STUN_HEADER_LEN + msg_len.min(buf.len().saturating_sub(STUN_HEADER_LEN));
    let mut offset = STUN_HEADER_LEN;
    let mut nonce = None;
    let mut realm = None;
    let mut relayed = None;
    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let attr_len = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start + attr_len;
        if value_end > buf.len() {
            break;
        }
        match attr_type {
            ATTR_NONCE => nonce = Some(String::from_utf8_lossy(&buf[value_start..value_end]).to_string()),
            ATTR_REALM => realm = Some(String::from_utf8_lossy(&buf[value_start..value_end]).to_string()),
            ATTR_XOR_RELAYED_ADDRESS => {
                if let Ok(addr) = decode_xor_addr(&buf[value_start..value_end], tx_id) {
                    relayed = Some(addr);
                }
            }
            _ => {}
        }
        offset = value_start + ((attr_len + 3) & !3);
    }

    match msg_type {
        TURN_ALLOCATE_RESPONSE | TURN_CREATE_PERMISSION_RESPONSE | TURN_CHANNEL_BIND_RESPONSE => {
            let relayed = relayed.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
            Ok(TurnResponse { kind: TurnResponseKind::Success { relayed } })
        }
        TURN_ALLOCATE_ERROR => {
            if let (Some(nonce), Some(realm)) = (nonce, realm) {
                Ok(TurnResponse { kind: TurnResponseKind::AuthChallenge { nonce, realm } })
            } else {
                Ok(TurnResponse { kind: TurnResponseKind::Error })
            }
        }
        _ => Ok(TurnResponse { kind: TurnResponseKind::Error }),
    }
}

fn parse_data_indication(buf: &[u8]) -> Result<(SocketAddr, &[u8]), TurnError> {
    if buf.len() < STUN_HEADER_LEN {
        return Err(TurnError::InvalidResponse);
    }
    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    if msg_type != TURN_DATA_INDICATION {
        return Err(TurnError::InvalidResponse);
    }
    let tx_id: [u8; 12] = buf[8..20].try_into().map_err(|_| TurnError::InvalidResponse)?;
    let msg_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let end = STUN_HEADER_LEN + msg_len.min(buf.len().saturating_sub(STUN_HEADER_LEN));
    let mut offset = STUN_HEADER_LEN;
    let mut peer = None;
    let mut data = None;
    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let attr_len = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start + attr_len;
        if value_end > buf.len() {
            break;
        }
        match attr_type {
            ATTR_XOR_PEER_ADDRESS => {
                peer = decode_xor_addr(&buf[value_start..value_end], &tx_id).ok();
            }
            ATTR_DATA => {
                data = Some(&buf[value_start..value_end]);
            }
            _ => {}
        }
        offset = value_start + ((attr_len + 3) & !3);
    }
    let peer = peer.ok_or(TurnError::InvalidResponse)?;
    let data = data.ok_or(TurnError::InvalidResponse)?;
    Ok((peer, data))
}

fn build_stun_header(msg_type: u16, tx_id: &[u8; 12]) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    out.extend_from_slice(&msg_type.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    out.extend_from_slice(tx_id);
    out
}

fn add_attr_u32(buf: &mut Vec<u8>, attr: u16, value: u32) {
    buf.extend_from_slice(&attr.to_be_bytes());
    buf.extend_from_slice(&4u16.to_be_bytes());
    buf.extend_from_slice(&value.to_be_bytes());
}

fn add_attr_bytes(buf: &mut Vec<u8>, attr: u16, value: &[u8]) {
    buf.extend_from_slice(&attr.to_be_bytes());
    buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
    buf.extend_from_slice(value);
    let pad = (4 - (value.len() % 4)) % 4;
    for _ in 0..pad {
        buf.push(0);
    }
}

fn finalize_length(buf: &mut Vec<u8>) {
    let len = buf.len().saturating_sub(STUN_HEADER_LEN) as u16;
    buf[2..4].copy_from_slice(&len.to_be_bytes());
}

fn add_message_integrity(buf: &mut Vec<u8>, username: &str, realm: &str, password: &str) {
    finalize_length(buf);
    let key = format!("{}:{}:{}", username, realm, password);
    let mut mac = HmacSha1::new_from_slice(key.as_bytes()).expect("hmac key");
    mac.update(buf);
    let result = mac.finalize().into_bytes();
    add_attr_bytes(buf, ATTR_MESSAGE_INTEGRITY, &result);
    finalize_length(buf);
}

fn add_fingerprint(buf: &mut Vec<u8>) {
    finalize_length(buf);
    let mut hasher = Crc32::new();
    hasher.update(buf);
    let crc = hasher.finalize() ^ 0x5354_554e;
    add_attr_u32(buf, ATTR_FINGERPRINT, crc);
    finalize_length(buf);
}

fn encode_xor_addr(addr: SocketAddr, tx_id: &[u8; 12]) -> Vec<u8> {
    match addr {
        SocketAddr::V4(addr) => {
            let port = addr.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
            let ip = u32::from(*addr.ip()) ^ STUN_MAGIC_COOKIE;
            let mut out = Vec::with_capacity(8);
            out.push(0);
            out.push(0x01);
            out.extend_from_slice(&port.to_be_bytes());
            out.extend_from_slice(&ip.to_be_bytes());
            out
        }
        SocketAddr::V6(addr) => {
            let port = addr.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
            let mut ip = addr.ip().octets();
            let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                ip[i] ^= cookie[i];
            }
            for i in 0..12 {
                ip[4 + i] ^= tx_id[i];
            }
            let mut out = Vec::with_capacity(20);
            out.push(0);
            out.push(0x02);
            out.extend_from_slice(&port.to_be_bytes());
            out.extend_from_slice(&ip);
            out
        }
    }
}

fn decode_xor_addr(buf: &[u8], tx_id: &[u8; 12]) -> Result<SocketAddr, TurnError> {
    if buf.len() < 4 {
        return Err(TurnError::InvalidResponse);
    }
    let family = buf[1];
    let port = u16::from_be_bytes([buf[2], buf[3]]) ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
    match family {
        0x01 => {
            if buf.len() < 8 {
                return Err(TurnError::InvalidResponse);
            }
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&buf[4..8]);
            let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                ip[i] ^= cookie[i];
            }
            Ok(SocketAddr::new(IpAddr::V4(ip.into()), port))
        }
        0x02 => {
            if buf.len() < 20 {
                return Err(TurnError::InvalidResponse);
            }
            let mut ip = [0u8; 16];
            ip.copy_from_slice(&buf[4..20]);
            let mut xor = [0u8; 16];
            xor[..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            xor[4..].copy_from_slice(tx_id);
            for i in 0..16 {
                ip[i] ^= xor[i];
            }
            Ok(SocketAddr::new(IpAddr::V6(ip.into()), port))
        }
        _ => Err(TurnError::InvalidResponse),
    }
}

fn random_tx_id() -> [u8; 12] {
    let mut tx_id = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut tx_id);
    tx_id
}

fn is_channel_data(buf: &[u8]) -> bool {
    if buf.len() < 4 {
        return false;
    }
    let channel = u16::from_be_bytes([buf[0], buf[1]]);
    (0x4000..=0x7FFF).contains(&channel)
}

async fn allocate_channel_number(channels: &Mutex<HashMap<SocketAddr, u16>>) -> u16 {
    let mut num = 0x4000u16;
    let existing = channels.lock().await.values().copied().collect::<HashSet<_>>();
    while existing.contains(&num) {
        num = num.wrapping_add(1);
        if num < 0x4000 {
            num = 0x4000;
        }
    }
    num
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_turn_uri_defaults() {
        let cfg = parse_turn_server("turn:127.0.0.1").unwrap();
        assert_eq!(cfg.addr.port(), TURN_DEFAULT_PORT);
    }

    #[tokio::test]
    async fn allocate_turn_no_auth() {
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 34790);
        let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 50000);

        let _server = tokio::spawn(async move {
            let socket = UdpSocket::bind(server_addr).await.unwrap();
            let mut buf = [0u8; 1500];
            let (len, peer) = socket.recv_from(&mut buf).await.unwrap();
            let tx_id: [u8; 12] = buf[8..20].try_into().unwrap();
            let mut response = build_stun_header(TURN_ALLOCATE_RESPONSE, &tx_id);
            add_attr_bytes(&mut response, ATTR_XOR_RELAYED_ADDRESS, &encode_xor_addr(relay_addr, &tx_id));
            finalize_length(&mut response);
            let _ = socket.send_to(&response, peer).await;
        });

        let cfg = TurnServerConfig {
            addr: server_addr,
            username: None,
            credential: None,
        };
        let cand = allocate_turn_relay(cfg, 1000).await.unwrap();
        assert_eq!(cand.relay_addr, relay_addr);
    }
}
