use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, Result};
use rift_core::Identity;
use rift_mesh::{Mesh, MeshConfig, MeshEvent};
use rift_metrics as metrics;
use rift_protocol::QosProfile;
use tokio::sync::mpsc;
use tokio::time::timeout;

const E2E_TIMEOUT: Duration = Duration::from_secs(8);

struct MeshHarness {
    handle: rift_mesh::MeshHandle,
    rx: mpsc::UnboundedReceiver<MeshEvent>,
    peer_id: rift_core::PeerId,
    addr: SocketAddr,
}

async fn spawn_mesh(name: &str) -> Result<MeshHarness> {
    let identity = Identity::generate();
    let peer_id = identity.peer_id;
    let cfg = MeshConfig {
        channel_name: format!("e2e-{name}"),
        password: None,
        listen_port: 0,
        relay_capable: false,
        qos: QosProfile::default(),
        auth_token: None,
        require_auth: false,
        e2ee_key: Some([7u8; 32]),
        rekey_interval_secs: None,
        max_direct_peers: None,
    };
    let mut mesh = Mesh::new(identity, cfg).await?;
    let addr = mesh.local_addr()?;
    let handle = mesh.handle();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(event) = mesh.next_event().await {
            let _ = tx.send(event);
        }
    });
    Ok(MeshHarness {
        handle,
        rx,
        peer_id,
        addr,
    })
}

async fn wait_for_event<F>(rx: &mut mpsc::UnboundedReceiver<MeshEvent>, mut predicate: F) -> Result<MeshEvent>
where
    F: FnMut(&MeshEvent) -> bool,
{
    let evt = timeout(E2E_TIMEOUT, async {
        while let Some(evt) = rx.recv().await {
            if predicate(&evt) {
                return Some(evt);
            }
        }
        None
    })
    .await
    .map_err(|_| anyhow!("timeout waiting for event"))?;
    evt.ok_or_else(|| anyhow!("event channel closed"))
}

#[tokio::test]
async fn lan_direct_chat() -> Result<()> {
    metrics::set_enabled(true);
    let mut a = spawn_mesh("a").await?;
    let mut b = spawn_mesh("b").await?;

    b.handle.connect_addr(a.addr).await?;

    let _ = wait_for_event(&mut a.rx, |e| matches!(e, MeshEvent::PeerJoined(_))).await?;
    let _ = wait_for_event(&mut b.rx, |e| matches!(e, MeshEvent::PeerJoined(_))).await?;

    a.handle.broadcast_chat("hello".to_string()).await?;
    let evt = wait_for_event(&mut b.rx, |e| matches!(e, MeshEvent::ChatReceived(_))).await?;
    if let MeshEvent::ChatReceived(chat) = evt {
        assert_eq!(chat.text, "hello");
        assert_eq!(chat.from, a.peer_id);
    }

    let snap = metrics::snapshot();
    assert!(!snap.counters.is_empty());
    Ok(())
}

#[tokio::test]
async fn lan_voice_frame() -> Result<()> {
    let mut a = spawn_mesh("va").await?;
    let mut b = spawn_mesh("vb").await?;

    b.handle.connect_addr(a.addr).await?;
    let _ = wait_for_event(&mut a.rx, |e| matches!(e, MeshEvent::PeerJoined(_))).await?;
    let _ = wait_for_event(&mut b.rx, |e| matches!(e, MeshEvent::PeerJoined(_))).await?;

    let payload = vec![1u8, 2, 3, 4];
    a.handle.broadcast_voice(1, 1234, payload.clone()).await?;
    let evt = wait_for_event(&mut b.rx, |e| matches!(e, MeshEvent::VoiceFrame { .. })).await?;
    if let MeshEvent::VoiceFrame { payload: got, .. } = evt {
        assert_eq!(got, payload);
    }
    Ok(())
}

#[tokio::test]
async fn group_mesh_to_hybrid_topology() -> Result<()> {
    let mut meshes = Vec::new();
    for i in 0..6 {
        meshes.push(spawn_mesh(&format!("g{i}")).await?);
    }
    let leader_addr = meshes[0].addr;

    for idx in 1..meshes.len() {
        meshes[idx].handle.connect_addr(leader_addr).await?;
    }

    let evt = wait_for_event(&mut meshes[0].rx, |e| {
        matches!(e, MeshEvent::GroupTopology { .. })
    }).await?;

    if let MeshEvent::GroupTopology { mode, .. } = evt {
        match mode {
            rift_protocol::GroupMode::Hybrid { .. } => {}
            rift_protocol::GroupMode::Mesh => return Err(anyhow!("expected hybrid mode")),
        }
    }

    Ok(())
}

#[tokio::test]
#[ignore]
async fn nat_restrictive_turn_fallback() -> Result<()> {
    // Requires TURN server and NAT simulation.
    Ok(())
}

#[tokio::test]
#[ignore]
async fn stun_srflx_connectivity() -> Result<()> {
    // Requires STUN servers reachable in CI.
    Ok(())
}
