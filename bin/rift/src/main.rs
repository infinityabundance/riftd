use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use rift_core::message::{decode_message, encode_message, CoreMessage};
use rift_core::noise::{noise_builder, NoiseSession};
use rift_core::{Identity, PeerId};
use tokio::net::UdpSocket;

/*
Usage:

# terminal 1
$ rift init-identity
$ rift listen --port 7777

# terminal 2
$ rift init-identity
$ rift send --to 192.168.1.10:7777 --text "hello rift"
*/

const MAX_PACKET: usize = 2048;

#[derive(Parser, Debug)]
#[command(name = "rift", version, about = "Minimal P2P chat over UDP + Noise XX")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    InitIdentity,
    Listen {
        #[arg(long)]
        port: u16,
    },
    Send {
        #[arg(long)]
        to: String,
        #[arg(long)]
        text: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::InitIdentity => cmd_init_identity().await,
        Commands::Listen { port } => cmd_listen(port).await,
        Commands::Send { to, text } => cmd_send(&to, &text).await,
    }
}

async fn cmd_init_identity() -> Result<()> {
    let path = Identity::default_path()?;
    match Identity::load(Some(&path)) {
        Ok(_) => {
            println!("Identity already exists at {}", path.display());
            Ok(())
        }
        Err(rift_core::CoreError::IdentityMissing(_)) => {
            let identity = Identity::generate();
            identity.save(&path)?;
            println!("Generated identity at {}", path.display());
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

async fn cmd_listen(port: u16) -> Result<()> {
    let identity = Identity::load(None)?;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let socket = UdpSocket::bind(addr).await?;
    println!("Listening on {}", socket.local_addr()?);

    let mut buf = [0u8; MAX_PACKET];
    let (len, peer_addr) = socket.recv_from(&mut buf).await?;

    let mut session = handshake_responder(&socket, &buf[..len], peer_addr).await?;
    println!("Handshake completed with {}", peer_addr);

    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        if addr != peer_addr {
            continue;
        }
        let mut plaintext = [0u8; MAX_PACKET];
        let out_len = session.decrypt(&buf[..len], &mut plaintext)?;
        let msg = decode_message(&plaintext[..out_len])?;
        if let CoreMessage::Chat { from, text, .. } = msg {
            println!("[from={}] {}", from, text);
        }
    }
}

async fn cmd_send(to: &str, text: &str) -> Result<()> {
    let identity = Identity::load(None)?;
    let peer_addr: SocketAddr = to.parse().context("invalid --to address")?;
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let mut session = handshake_initiator(&socket, peer_addr).await?;

    let msg = CoreMessage::Chat {
        from: identity.peer_id,
        nonce: now_nonce(),
        text: text.to_string(),
    };
    let plaintext = encode_message(&msg);
    let mut out = vec![0u8; plaintext.len() + 128];
    let len = session.encrypt(&plaintext, &mut out)?;
    socket.send_to(&out[..len], peer_addr).await?;
    Ok(())
}

fn now_nonce() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

async fn handshake_initiator(socket: &UdpSocket, peer: SocketAddr) -> Result<NoiseSession> {
    let builder = noise_builder();
    let static_kp = builder.generate_keypair()?;
    let mut hs = builder
        .local_private_key(&static_kp.private)
        .build_initiator()?;

    let mut buf = [0u8; MAX_PACKET];
    let len = hs.write_message(&[], &mut buf)?;
    socket.send_to(&buf[..len], peer).await?;

    let mut in_buf = [0u8; MAX_PACKET];
    let (len, addr) = socket.recv_from(&mut in_buf).await?;
    if addr != peer {
        return Err(anyhow!("unexpected peer during handshake"));
    }
    let mut out_buf = [0u8; MAX_PACKET];
    hs.read_message(&in_buf[..len], &mut out_buf)?;

    let len = hs.write_message(&[], &mut buf)?;
    socket.send_to(&buf[..len], peer).await?;

    let transport = hs.into_transport_mode()?;
    Ok(NoiseSession::new(transport))
}

async fn handshake_responder(
    socket: &UdpSocket,
    first_msg: &[u8],
    peer: SocketAddr,
) -> Result<NoiseSession> {
    let builder = noise_builder();
    let static_kp = builder.generate_keypair()?;
    let mut hs = builder
        .local_private_key(&static_kp.private)
        .build_responder()?;

    let mut out_buf = [0u8; MAX_PACKET];
    hs.read_message(first_msg, &mut out_buf)?;

    let mut buf = [0u8; MAX_PACKET];
    let len = hs.write_message(&[], &mut buf)?;
    socket.send_to(&buf[..len], peer).await?;

    let mut in_buf = [0u8; MAX_PACKET];
    let (len, addr) = socket.recv_from(&mut in_buf).await?;
    if addr != peer {
        return Err(anyhow!("unexpected peer during handshake"));
    }
    hs.read_message(&in_buf[..len], &mut out_buf)?;

    let transport = hs.into_transport_mode()?;
    Ok(NoiseSession::new(transport))
}
