use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncBufReadExt, BufReader};

use rift_core::Identity;
use rift_mesh::{Mesh, MeshConfig, MeshEvent};

/*
Usage:

# terminal 1
$ rift init-identity
$ rift create --channel gaming

# terminal 2 (same LAN)
$ rift init-identity
$ rift create --channel gaming
*/

#[derive(Parser, Debug)]
#[command(name = "rift", version, about = "LAN-only P2P chat over UDP + Noise + mDNS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    InitIdentity,
    Create {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, default_value_t = 7777)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::InitIdentity => cmd_init_identity().await,
        Commands::Create {
            channel,
            password,
            port,
        } => cmd_create(channel, password, port).await,
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

async fn cmd_create(channel: String, password: Option<String>, port: u16) -> Result<()> {
    let identity = Identity::load(None).context("identity not found, run init-identity first")?;

    let config = MeshConfig {
        channel_name: channel,
        password,
        listen_port: port,
    };

    let mut mesh = Mesh::new(identity, config).await?;
    mesh.start_discovery()?;
    println!("Listening on {}", mesh.local_addr()?);
    println!("Type a message and press enter to send.");

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                let text = line.trim();
                if text.is_empty() {
                    continue;
                }
                mesh.broadcast_chat(text.to_string()).await?;
            }
            event = mesh.next_event() => {
                let Some(event) = event else { break; };
                match event {
                    MeshEvent::PeerJoined(peer_id) => {
                        println!("[peer {} joined]", peer_id);
                    }
                    MeshEvent::PeerLeft(peer_id) => {
                        println!("[peer {} left]", peer_id);
                    }
                    MeshEvent::ChatReceived(chat) => {
                        println!("[from={}] {}", chat.from, chat.text);
                    }
                }
            }
        }
    }

    Ok(())
}
