use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{info, warn};

#[derive(Clone, Default)]
struct RelayState {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase", tag = "type")]
enum RelayEnvelope {
    Join { room: String, peer_id: String },
    Data {
        room: String,
        peer_id: String,
        data: String,
    },
    Status { room: String, peer_id: String, status: String },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let state = RelayState::default();
    let app = Router::new().route("/ws", get(ws_handler)).with_state(state);
    let addr: SocketAddr = ([0, 0, 0, 0], 8787).into();
    info!(%addr, "rift ws relay listening");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<RelayState>) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: RelayState) {
    let (mut sender, mut receiver) = socket.split();

    let join_msg = match receiver.next().await {
        Some(Ok(Message::Text(text))) => text,
        Some(Ok(_)) => {
            let _ = sender
                .send(Message::Close(None))
                .await;
            return;
        }
        _ => return,
    };

    let join = match serde_json::from_str::<RelayEnvelope>(&join_msg) {
        Ok(RelayEnvelope::Join { room, peer_id }) => (room, peer_id),
        _ => {
            let _ = sender
                .send(Message::Close(None))
                .await;
            return;
        }
    };

    let (room, peer_id) = join;
    info!(room = %room, peer_id = %peer_id, "peer joined");

    let tx = room_sender(&state, &room);
    let mut rx = tx.subscribe();
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let tx_clone = tx.clone();
    let room_clone = room.clone();
    let peer_clone = peer_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            let Message::Text(text) = msg else {
                continue;
            };
            match serde_json::from_str::<RelayEnvelope>(&text) {
                Ok(RelayEnvelope::Data { room, .. }) if room == room_clone => {
                    let _ = tx_clone.send(text);
                }
                Ok(RelayEnvelope::Status { room, .. }) if room == room_clone => {
                    let _ = tx_clone.send(text);
                }
                Ok(RelayEnvelope::Join { .. }) => {
                    warn!(room = %room_clone, peer_id = %peer_clone, "duplicate join ignored");
                }
                _ => {
                    warn!(room = %room_clone, peer_id = %peer_clone, "ignored malformed message");
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        }
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }

    info!(room = %room, peer_id = %peer_id, "peer left");
}

fn room_sender(state: &RelayState, room: &str) -> broadcast::Sender<String> {
    let mut rooms = state.rooms.lock().expect("rooms lock");
    rooms
        .entry(room.to_string())
        .or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(256);
            tx
        })
        .clone()
}
