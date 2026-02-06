use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncBufReadExt, BufReader};

use rift_core::{decode_invite, encode_invite, generate_invite, Identity};
use rift_media::{AudioConfig, AudioIn, AudioMixer, AudioOut, OpusDecoder, OpusEncoder};
use rift_mesh::{Mesh, MeshConfig, MeshEvent};
use rift_nat::NatConfig;

/*
Usage:

# terminal 1
$ rift init-identity
$ rift create --channel gaming

# terminal 2 (same LAN)
$ rift init-identity
$ rift create --channel gaming

# voice mode
$ rift create --channel gaming --voice

# internet mode
$ rift create --channel gaming --internet
$ rift invite --channel gaming
$ rift join --invite "rift://join/..."
*/

#[derive(Parser, Debug)]
#[command(name = "rift", version, about = "P2P chat over UDP + Noise + mDNS")]
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
        #[arg(long)]
        voice: bool,
        #[arg(long)]
        internet: bool,
    },
    Invite {
        #[arg(long)]
        channel: String,
    },
    Join {
        #[arg(long)]
        invite: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        voice: bool,
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
            voice,
            internet,
        } => cmd_create(channel, password, port, voice, internet).await,
        Commands::Invite { channel } => cmd_invite(channel).await,
        Commands::Join { invite, port, voice } => cmd_join(invite, port, voice).await,
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

async fn cmd_create(
    channel: String,
    password: Option<String>,
    port: u16,
    voice: bool,
    internet: bool,
) -> Result<()> {
    let identity = Identity::load(None).context("identity not found, run init-identity first")?;

    let config = MeshConfig {
        channel_name: channel.clone(),
        password: password.clone(),
        listen_port: port,
    };

    let mut mesh = Mesh::new(identity, config).await?;

    if internet {
        let invite = generate_invite(&channel, password.as_deref(), Vec::new());
        let invite_str = encode_invite(&invite);
        save_invite_string(&channel, &invite_str)?;
        mesh.enable_nat(default_nat_config(port)).await;
        println!("Invite saved. Use `rift invite --channel {}` to print it.", channel);
    } else {
        mesh.start_lan_discovery()?;
    }

    run_chat_loop(mesh, voice).await
}

async fn cmd_invite(channel: String) -> Result<()> {
    let invite = load_invite_string(&channel)
        .with_context(|| format!("invite for channel '{}' not found", channel))?;
    println!("{}", invite);
    Ok(())
}

async fn cmd_join(invite_str: String, port: u16, voice: bool) -> Result<()> {
    let invite = decode_invite(&invite_str)?;
    let identity = Identity::load(None).context("identity not found, run init-identity first")?;

    let config = MeshConfig {
        channel_name: invite.channel_name.clone(),
        password: invite.password.clone(),
        listen_port: port,
    };

    let mut mesh = Mesh::new(identity, config).await?;
    let nat_cfg = default_nat_config(port);
    mesh.join_invite(invite, nat_cfg).await?;

    run_chat_loop(mesh, voice).await
}

async fn run_chat_loop(mut mesh: Mesh, voice: bool) -> Result<()> {
    println!("Listening on {}", mesh.local_addr()?);
    println!("Type a message and press enter to send.");

    let (_audio_in, mut audio_rx, mut opus_enc, mut opus_dec, audio_out, mixer) = if voice {
        let audio_config = AudioConfig::default();
        let (audio_in, audio_rx) = AudioIn::new(&audio_config)?;
        let opus_enc = OpusEncoder::new(&audio_config)?;
        let opus_dec = OpusDecoder::new(&audio_config)?;
        let audio_out = AudioOut::new(&audio_config)?;
        let mixer = Arc::new(Mutex::new(AudioMixer::new(audio_config.frame_samples())));
        (
            Some(audio_in),
            Some(audio_rx),
            Some(opus_enc),
            Some(opus_dec),
            Some(audio_out),
            Some(mixer),
        )
    } else {
        (None, None, None, None, None, None)
    };

    if voice {
        let mesh_handle = mesh.handle();
        let mut encoder = opus_enc.take().unwrap();
        let mut audio_rx = audio_rx.take().unwrap();
        tokio::spawn(async move {
            let mut seq: u32 = 0;
            while let Some(frame) = audio_rx.recv().await {
                let mut out = vec![0u8; 4000];
                let len = match encoder.encode_i16(&frame, &mut out) {
                    Ok(len) => len,
                    Err(err) => {
                        eprintln!("opus encode error: {err}");
                        continue;
                    }
                };
                out.truncate(len);
                let timestamp = now_timestamp();
                if let Err(err) = mesh_handle.broadcast_voice(seq, timestamp, out).await {
                    eprintln!("voice send error: {err}");
                }
                seq = seq.wrapping_add(1);
            }
        });

        let audio_out = audio_out.unwrap();
        let mixer = mixer.clone().unwrap();
        let frame_samples = audio_out.frame_samples();
        let frame_duration = AudioConfig::default().frame_duration();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(frame_duration).await;
                let frame = {
                    let mut mixer = mixer.lock().unwrap();
                    mixer.mix_next()
                };
                if frame.len() == frame_samples {
                    audio_out.push_frame(&frame);
                }
            }
        });
    }

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    let mut decoder = opus_dec;
    let mixer = mixer;

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
                    MeshEvent::VoiceFrame { from, payload, .. } => {
                        if let (Some(decoder), Some(mixer)) = (decoder.as_mut(), mixer.as_ref()) {
                            let mut out = vec![0i16; AudioConfig::default().frame_samples()];
                            match decoder.decode_i16(&payload, &mut out) {
                                Ok(len) => {
                                    out.truncate(len);
                                    let stream_id = peer_to_stream_id(&from);
                                    let mut mixer = mixer.lock().unwrap();
                                    mixer.push(stream_id, out);
                                }
                                Err(err) => {
                                    eprintln!("opus decode error: {err}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn peer_to_stream_id(peer: &rift_core::PeerId) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&peer.0[..8]);
    u64::from_le_bytes(bytes)
}

fn invite_path(channel: &str) -> Result<PathBuf> {
    let base = dirs::config_dir().context("config directory not found")?;
    Ok(base.join("rift").join("invites").join(format!("{channel}.txt")))
}

fn save_invite_string(channel: &str, invite: &str) -> Result<()> {
    let path = invite_path(channel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, invite)?;
    Ok(())
}

fn load_invite_string(channel: &str) -> Result<String> {
    let path = invite_path(channel)?;
    let content = fs::read_to_string(path)?;
    Ok(content)
}

fn default_nat_config(port: u16) -> NatConfig {
    let mut ports = Vec::new();
    ports.push(port);
    ports.push(port.saturating_add(1));
    ports.push(port.saturating_add(2));
    NatConfig { local_ports: ports }
}
