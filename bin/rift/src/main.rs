use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncBufReadExt, BufReader};

use rift_core::Identity;
use rift_media::{AudioConfig, AudioIn, AudioMixer, AudioOut, OpusDecoder, OpusEncoder};
use rift_mesh::{Mesh, MeshConfig, MeshEvent};

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
        } => cmd_create(channel, password, port, voice).await,
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

async fn cmd_create(channel: String, password: Option<String>, port: u16, voice: bool) -> Result<()> {
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

    let (audio_in, mut audio_rx, mut opus_enc, mut opus_dec, audio_out, mixer) = if voice {
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

    drop(audio_in);

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
