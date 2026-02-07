use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::task::LocalSet;

use rift_core::{decode_invite, encode_invite, generate_invite, Identity};
use rift_sdk::{AudioConfigSdk, NetworkConfigSdk, RiftConfig, RiftEvent, RiftHandle, RiftSessionId};

mod config;
use config::UserConfig;

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
        #[arg(long)]
        relay: bool,
    },
    Call {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, default_value_t = 7777)]
        port: u16,
        #[arg(long)]
        voice: bool,
        #[arg(long)]
        internet: bool,
        #[arg(long)]
        relay: bool,
        #[arg(long)]
        invite: Option<String>,
    },
    Accept {
        #[arg(long)]
        session: String,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, default_value_t = 7777)]
        port: u16,
        #[arg(long)]
        voice: bool,
        #[arg(long)]
        internet: bool,
        #[arg(long)]
        relay: bool,
        #[arg(long)]
        invite: Option<String>,
    },
    Decline {
        #[arg(long)]
        session: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, default_value_t = 7777)]
        port: u16,
        #[arg(long)]
        voice: bool,
        #[arg(long)]
        internet: bool,
        #[arg(long)]
        relay: bool,
        #[arg(long)]
        invite: Option<String>,
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
        #[arg(long)]
        relay: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Commands::InitIdentity => cmd_init_identity().await,
        Commands::Create {
            channel,
            password,
            port,
            voice,
            internet,
            relay,
        } => cmd_create(channel, password, port, voice, internet, relay, StartupAction::None).await,
        Commands::Call {
            peer,
            channel,
            password,
            port,
            voice,
            internet,
            relay,
            invite,
        } => {
            let action = StartupAction::Call { peer };
            if let Some(invite) = invite {
                cmd_join(invite, port, voice, relay, action).await
            } else {
                let channel = channel.context("--channel is required without --invite")?;
                cmd_create(channel, password, port, voice, internet, relay, action).await
            }
        }
        Commands::Accept {
            session,
            channel,
            password,
            port,
            voice,
            internet,
            relay,
            invite,
        } => {
            let session = parse_session_id(&session)?;
            let action = StartupAction::Accept { session };
            if let Some(invite) = invite {
                cmd_join(invite, port, voice, relay, action).await
            } else {
                let channel = channel.context("--channel is required without --invite")?;
                cmd_create(channel, password, port, voice, internet, relay, action).await
            }
        }
        Commands::Decline {
            session,
            reason,
            channel,
            password,
            port,
            voice,
            internet,
            relay,
            invite,
        } => {
            let session = parse_session_id(&session)?;
            let action = StartupAction::Decline { session, reason };
            if let Some(invite) = invite {
                cmd_join(invite, port, voice, relay, action).await
            } else {
                let channel = channel.context("--channel is required without --invite")?;
                cmd_create(channel, password, port, voice, internet, relay, action).await
            }
        }
        Commands::Invite { channel } => cmd_invite(channel).await,
        Commands::Join {
            invite,
            port,
            voice,
            relay,
        } => cmd_join(invite, port, voice, relay, StartupAction::None).await,
    }
}

fn init_logging() {
    let log_path = dirs::config_dir()
        .map(|base| base.join("rift").join("rift.log"));
    if let Some(path) = log_path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let writer = tracing_subscriber::fmt::writer::BoxMakeWriter::new(file);
            tracing_subscriber::fmt().with_writer(writer).init();
            return;
        }
    }
    tracing_subscriber::fmt::init();
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
    relay: bool,
    startup: StartupAction,
) -> Result<()> {
    let user_cfg = UserConfig::load()?;
    let relay_enabled = relay || user_cfg.network.relay.unwrap_or(false);

    if internet {
        let invite = generate_invite(&channel, password.as_deref(), Vec::new());
        let invite_str = encode_invite(&invite);
        save_invite_string(&channel, &invite_str)?;
    }

    let config = build_sdk_config(&user_cfg, port, relay_enabled, voice, None);
    let handle = RiftHandle::new(config).await?;
    handle
        .join_channel(&channel, password.as_deref(), internet)
        .await?;

    run_tui(handle, user_cfg, channel, password, startup).await
}

async fn cmd_invite(channel: String) -> Result<()> {
    let invite = load_invite_string(&channel)
        .with_context(|| format!("invite for channel '{}' not found", channel))?;
    println!("{}", invite);
    Ok(())
}

async fn cmd_join(
    invite_str: String,
    port: u16,
    voice: bool,
    relay: bool,
    startup: StartupAction,
) -> Result<()> {
    let invite = decode_invite(&invite_str)?;
    let user_cfg = UserConfig::load()?;
    let relay_enabled = relay || user_cfg.network.relay.unwrap_or(false);
    let channel_name = invite.channel_name.clone();
    let password = invite.password.clone();

    let config = build_sdk_config(&user_cfg, port, relay_enabled, voice, Some(invite_str));

    let handle = RiftHandle::new(config).await?;
    handle
        .join_channel(&channel_name, password.as_deref(), true)
        .await?;

    run_tui(handle, user_cfg, channel_name, password, startup).await
}

#[derive(Debug, Clone)]
struct ChatLine {
    timestamp: u64,
    name: String,
    text: String,
}

#[derive(Debug, Clone)]
struct PeerEntry {
    display: String,
    last_voice: Option<Instant>,
}

#[derive(Debug)]
enum UiEvent {
    Input(KeyEvent),
    Tick,
    Sdk(RiftEvent),
}

#[derive(Debug)]
enum UiAction {
    SendChat(String),
    StartCall(rift_core::PeerId),
    AcceptCall(RiftSessionId),
    DeclineCall(RiftSessionId, Option<String>),
    EndCall(RiftSessionId),
    ToggleMute,
}

#[derive(Debug, Clone)]
enum StartupAction {
    None,
    Call { peer: String },
    Accept { session: RiftSessionId },
    Decline { session: RiftSessionId, reason: Option<String> },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Focus {
    Input,
    Peers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PttKey {
    F(u8),
    Space,
    CtrlSpace,
    AltSpace,
    CtrlBacktick,
    CtrlSemicolon,
}

impl PttKey {
    fn from_config(value: Option<&str>) -> Self {
        match value.unwrap_or("f1") {
            "space" => PttKey::Space,
            "ctrl_space" => PttKey::CtrlSpace,
            "alt_space" => PttKey::AltSpace,
            "ctrl_backtick" => PttKey::CtrlBacktick,
            "ctrl_semicolon" => PttKey::CtrlSemicolon,
            v if v.starts_with('f') => {
                let n = v[1..].parse::<u8>().ok().filter(|n| (1..=12).contains(n));
                n.map(PttKey::F).unwrap_or(PttKey::F(1))
            }
            _ => PttKey::F(1),
        }
    }

    fn label(self) -> &'static str {
        match self {
            PttKey::F(1) => "F1",
            PttKey::F(2) => "F2",
            PttKey::F(3) => "F3",
            PttKey::F(4) => "F4",
            PttKey::F(5) => "F5",
            PttKey::F(6) => "F6",
            PttKey::F(7) => "F7",
            PttKey::F(8) => "F8",
            PttKey::F(9) => "F9",
            PttKey::F(10) => "F10",
            PttKey::F(11) => "F11",
            PttKey::F(12) => "F12",
            PttKey::Space => "Space",
            PttKey::CtrlSpace => "Ctrl+Space",
            PttKey::AltSpace => "Alt+Space",
            PttKey::CtrlBacktick => "Ctrl+`",
            PttKey::CtrlSemicolon => "Ctrl+;",
            PttKey::F(_) => "F1",
        }
    }
}

struct UiState {
    channel: String,
    input: String,
    focus: Focus,
    local_peer_id: rift_core::PeerId,
    local_display: String,
    channel_session: RiftSessionId,
    active_session: RiftSessionId,
    incoming_call: Option<(RiftSessionId, rift_core::PeerId)>,
    active_call_peer: Option<rift_core::PeerId>,
    pending_call: Option<(RiftSessionId, rift_core::PeerId)>,
    muted: bool,
    peers: HashMap<rift_core::PeerId, PeerEntry>,
    chat: VecDeque<ChatLine>,
    mic_active: bool,
    ptt_enabled: bool,
    ptt_key: PttKey,
    ptt_last_signal: Option<Instant>,
    last_tx: Option<Instant>,
    last_rx: Option<Instant>,
    user_name: String,
    audio_quality: String,
    theme: String,
    prefer_p2p: bool,
}

fn next_quality(current: &str) -> &'static str {
    match current {
        "low" => "medium",
        "medium" => "high",
        _ => "low",
    }
}

impl UiState {
    fn new(
        channel: String,
        ptt_enabled: bool,
        ptt_key: PttKey,
        user_name: String,
        audio_quality: String,
        theme: String,
        prefer_p2p: bool,
        local_peer_id: rift_core::PeerId,
        channel_session: RiftSessionId,
    ) -> Self {
        Self {
            channel,
            input: String::new(),
            focus: Focus::Input,
            local_peer_id,
            local_display: format!("{} ({})", user_name, short_peer(&local_peer_id)),
            channel_session,
            active_session: channel_session,
            incoming_call: None,
            active_call_peer: None,
            pending_call: None,
            muted: false,
            peers: HashMap::new(),
            chat: VecDeque::with_capacity(200),
            mic_active: false,
            ptt_enabled,
            ptt_key,
            ptt_last_signal: None,
            last_tx: None,
            last_rx: None,
            user_name,
            audio_quality,
            theme,
            prefer_p2p,
        }
    }

    fn add_chat_line(&mut self, name: String, text: String) {
        let line = ChatLine {
            timestamp: now_timestamp(),
            name,
            text,
        };
        if self.chat.len() >= 200 {
            self.chat.pop_front();
        }
        self.chat.push_back(line);
    }

    fn update_peer_voice(&mut self, peer_id: rift_core::PeerId) {
        let entry = self
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerEntry {
                display: short_peer(&peer_id),
                last_voice: None,
            });
        entry.last_voice = Some(Instant::now());
    }
}

async fn run_tui(
    handle: RiftHandle,
    user_cfg: UserConfig,
    channel: String,
    password: Option<String>,
    startup: StartupAction,
) -> Result<()> {
    let local = LocalSet::new();
    local
        .run_until(run_tui_inner(handle, user_cfg, channel, password, startup))
        .await
}

async fn run_tui_inner(
    handle: RiftHandle,
    user_cfg: UserConfig,
    channel: String,
    password: Option<String>,
    startup: StartupAction,
) -> Result<()> {
    let audio_quality = user_cfg.audio.quality.clone().unwrap_or_else(|| "medium".to_string());
    let ptt_enabled = user_cfg.audio.ptt.unwrap_or(false);
    let ptt_key = PttKey::from_config(user_cfg.audio.ptt_key.as_deref());
    let user_name = user_cfg
        .user
        .name
        .clone()
        .unwrap_or_else(|| "me".to_string());
    let theme = user_cfg.ui.theme.clone().unwrap_or_else(|| "dark".to_string());
    let prefer_p2p = user_cfg.network.prefer_p2p.unwrap_or(true);

    // tx/rx pulse timestamps tracked in UiState

    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<UiEvent>();
    let handle = Arc::new(handle);
    if ptt_enabled {
        handle.set_ptt_active(false);
    }

    let ui_tx_clone = ui_tx.clone();

    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(evt) = event::read() {
                if let Event::Key(key) = evt {
                    let _ = ui_tx_clone.send(UiEvent::Input(key));
                }
            }
        }
        let _ = ui_tx_clone.send(UiEvent::Tick);
    });

    let local_peer_id = handle.local_peer_id();
    let channel_session = RiftSessionId::from_channel(&channel, password.as_deref());
    let ui_tx_sdk = ui_tx.clone();
    let handle_events = handle.clone();
    tokio::task::spawn_local(async move {
        loop {
            if let Some(event) = handle_events.next_event().await {
                let _ = ui_tx_sdk.send(UiEvent::Sdk(event));
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = UiState::new(
        channel,
        ptt_enabled,
        ptt_key,
        user_name.clone(),
        audio_quality,
        theme,
        prefer_p2p,
        local_peer_id,
        channel_session,
    );
    let mut startup = startup;

    let mut should_quit = false;
    while !should_quit {
        terminal.draw(|f| draw_ui(f, &state))?;

        tokio::select! {
            Some(evt) = ui_rx.recv() => {
                match evt {
                    UiEvent::Input(key) => {
                        if let Some(action) = handle_key_event(
                            key,
                            &mut state,
                            handle.as_ref(),
                            &mut should_quit,
                        )? {
                            apply_action(action, &mut state, handle.as_ref()).await;
                        }
                    }
                    UiEvent::Tick => {
                        if state.ptt_enabled && state.mic_active {
                            if let Some(last) = state.ptt_last_signal {
                                if last.elapsed() > Duration::from_millis(400) {
                                    state.mic_active = false;
                                    handle.set_ptt_active(false);
                                }
                            }
                        }
                        if let Some(last) = state.last_tx {
                            if last.elapsed() > Duration::from_millis(300) {
                                state.last_tx = None;
                            }
                        }
                        if let Some(last) = state.last_rx {
                            if last.elapsed() > Duration::from_millis(300) {
                                state.last_rx = None;
                            }
                        }
                    }
                    UiEvent::Sdk(event) => {
                        match event {
                            RiftEvent::PeerJoinedChannel { peer, .. } => {
                                let peer_id = peer;
                                state.peers.entry(peer_id).or_insert(PeerEntry {
                                    display: short_peer(&peer_id),
                                    last_voice: None,
                                });
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::PeerLeftChannel { peer, .. } => {
                                let peer_id = peer;
                                state.peers.remove(&peer_id);
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::IncomingChat(chat) => {
                                state.add_chat_line(short_peer(&chat.from), chat.text);
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::AudioLevel { peer, level } => {
                                if level > 0.02 {
                                    state.update_peer_voice(peer);
                                }
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::VoiceFrame { .. } => {}
                            RiftEvent::IncomingCall { session, from } => {
                                state.incoming_call = Some((session, from));
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::CallStateChanged { session, state: rift_sdk::RiftCallState::Active } => {
                                state.active_session = session;
                                state.pending_call = None;
                                if let Some((incoming, from)) = state.incoming_call {
                                    if incoming == session {
                                        state.active_call_peer = Some(from);
                                        state.incoming_call = None;
                                    }
                                }
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::CallStateChanged { session, state: rift_sdk::RiftCallState::Ended } => {
                                if let Some((pending, _)) = state.pending_call {
                                    if pending == session {
                                        state.pending_call = None;
                                    }
                                }
                                if state.active_session == session {
                                    state.active_session = state.channel_session;
                                    state.active_call_peer = None;
                                }
                                state.last_rx = Some(Instant::now());
                            }
                            RiftEvent::CallStateChanged { .. } => {}
                        }
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                should_quit = true;
            }
        }

        if let Some(action) = take_startup_action(&mut startup, &state) {
            apply_action(action, &mut state, handle.as_ref()).await;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_key_event(
    key: KeyEvent,
    state: &mut UiState,
    handle: &RiftHandle,
    should_quit: &mut bool,
) -> Result<Option<UiAction>> {
    if state.ptt_enabled {
        let is_ptt = match state.ptt_key {
            PttKey::F(1) => key.code == KeyCode::F(1),
            PttKey::F(2) => key.code == KeyCode::F(2),
            PttKey::F(3) => key.code == KeyCode::F(3),
            PttKey::F(4) => key.code == KeyCode::F(4),
            PttKey::F(5) => key.code == KeyCode::F(5),
            PttKey::F(6) => key.code == KeyCode::F(6),
            PttKey::F(7) => key.code == KeyCode::F(7),
            PttKey::F(8) => key.code == KeyCode::F(8),
            PttKey::F(9) => key.code == KeyCode::F(9),
            PttKey::F(10) => key.code == KeyCode::F(10),
            PttKey::F(11) => key.code == KeyCode::F(11),
            PttKey::F(12) => key.code == KeyCode::F(12),
            PttKey::Space => key.code == KeyCode::Char(' '),
            PttKey::CtrlSpace => key.code == KeyCode::Char(' ')
                && key.modifiers.contains(KeyModifiers::CONTROL),
            PttKey::AltSpace => key.code == KeyCode::Char(' ')
                && key.modifiers.contains(KeyModifiers::ALT),
            PttKey::CtrlBacktick => key.code == KeyCode::Char('`')
                && key.modifiers.contains(KeyModifiers::CONTROL),
            PttKey::CtrlSemicolon => key.code == KeyCode::Char(';')
                && key.modifiers.contains(KeyModifiers::CONTROL),
            PttKey::F(_) => key.code == KeyCode::F(1),
        };
        if is_ptt {
            match key.kind {
                KeyEventKind::Press | KeyEventKind::Repeat => {
                    state.mic_active = true;
                    handle.set_ptt_active(true);
                    state.ptt_last_signal = Some(Instant::now());
                }
                KeyEventKind::Release => {
                    state.mic_active = false;
                    handle.set_ptt_active(false);
                    state.ptt_last_signal = None;
                }
            }
            return Ok(None);
        }
    }

    if let Some((session, _from)) = state.incoming_call {
        if key.code == KeyCode::Char('a') && key.modifiers.is_empty() {
            return Ok(Some(UiAction::AcceptCall(session)));
        }
        if key.code == KeyCode::Char('d') && key.modifiers.is_empty() {
            return Ok(Some(UiAction::DeclineCall(session, None)));
        }
    }

    match key.code {
        KeyCode::Tab => {
            state.focus = match state.focus {
                Focus::Input => Focus::Peers,
                Focus::Peers => Focus::Input,
            };
        }
        KeyCode::Char('q') => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                *should_quit = true;
            } else {
                state.input.push('q');
            }
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let next = next_quality(&state.audio_quality);
            state.audio_quality = next.to_string();
        }
        KeyCode::Char('m') if key.modifiers.is_empty() => {
            return Ok(Some(UiAction::ToggleMute));
        }
        KeyCode::Enter => {
            let text = state.input.trim().to_string();
            if !text.is_empty() {
                if let Some(rest) = text.strip_prefix("/call ") {
                    if let Some(peer_id) = resolve_peer_input(state, rest.trim()) {
                        state.input.clear();
                        return Ok(Some(UiAction::StartCall(peer_id)));
                    } else {
                        state.add_chat_line("system".to_string(), "unknown peer".to_string());
                    }
                } else if text == "/hangup" || text == "/bye" {
                    if state.active_session != state.channel_session {
                        let session = state.active_session;
                        state.input.clear();
                        return Ok(Some(UiAction::EndCall(session)));
                    }
                } else {
                    state.input.clear();
                    return Ok(Some(UiAction::SendChat(text)));
                }
            }
            state.input.clear();
        }
        KeyCode::Backspace => {
            state.input.pop();
        }
        KeyCode::Char(c) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                state.input.push(c);
            }
        }
        _ => {}
    }
    Ok(None)
}

async fn apply_action(
    action: UiAction,
    state: &mut UiState,
    handle: &RiftHandle,
) {
    match action {
        UiAction::SendChat(text) => {
            state.add_chat_line(state.user_name.clone(), text.clone());
            if let Err(err) = handle.send_chat(&text).await {
                tracing::debug!("chat send error: {err}");
            }
            state.last_tx = Some(Instant::now());
        }
        UiAction::StartCall(peer_id) => {
            if let Ok(session) = handle.start_call(peer_id).await {
                state.active_call_peer = Some(peer_id);
                state.incoming_call = None;
                state.pending_call = Some((session, peer_id));
                state.last_tx = Some(Instant::now());
            }
        }
        UiAction::AcceptCall(session) => {
            if let Err(err) = handle.accept_call(session).await {
                tracing::debug!("accept call error: {err}");
            } else {
                if let Some((incoming, from)) = state.incoming_call {
                    if incoming == session {
                        state.active_call_peer = Some(from);
                    }
                }
                state.incoming_call = None;
                state.pending_call = None;
                state.active_session = session;
                state.last_tx = Some(Instant::now());
            }
        }
        UiAction::DeclineCall(session, reason) => {
            if let Err(err) = handle.decline_call(session, reason.as_deref()).await {
                tracing::debug!("decline call error: {err}");
            } else {
                state.incoming_call = None;
                if let Some((pending, _)) = state.pending_call {
                    if pending == session {
                        state.pending_call = None;
                    }
                }
                if state.active_session == session {
                    state.active_session = state.channel_session;
                    state.active_call_peer = None;
                }
                state.last_tx = Some(Instant::now());
            }
        }
        UiAction::EndCall(session) => {
            if let Err(err) = handle.end_call(session).await {
                tracing::debug!("end call error: {err}");
            } else if state.active_session == session {
                state.active_session = state.channel_session;
                state.active_call_peer = None;
                state.last_tx = Some(Instant::now());
            }
        }
        UiAction::ToggleMute => {
            state.muted = !state.muted;
            handle.set_mute(state.muted);
        }
    }
}

fn take_startup_action(startup: &mut StartupAction, state: &UiState) -> Option<UiAction> {
    match startup {
        StartupAction::None => None,
        StartupAction::Call { peer } => {
            if let Some(peer_id) = resolve_peer_input(state, peer) {
                *startup = StartupAction::None;
                Some(UiAction::StartCall(peer_id))
            } else {
                None
            }
        }
        StartupAction::Accept { session } => {
            if let Some((incoming, _)) = state.incoming_call {
                if incoming == *session {
                    let session = *session;
                    *startup = StartupAction::None;
                    return Some(UiAction::AcceptCall(session));
                }
            }
            None
        }
        StartupAction::Decline { session, reason } => {
            if let Some((incoming, _)) = state.incoming_call {
                if incoming == *session {
                    let session = *session;
                    let reason = reason.clone();
                    *startup = StartupAction::None;
                    return Some(UiAction::DeclineCall(session, reason));
                }
            }
            None
        }
    }
}

fn draw_ui(f: &mut Frame, state: &UiState) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2), Constraint::Length(3)])
        .split(size);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(1)])
        .split(chunks[0]);

    draw_peers(f, body[0], state);
    draw_chat(f, body[1], state);
    draw_status(f, chunks[1], state);
    draw_input(f, chunks[2], state);
    if state.focus == Focus::Input {
        set_input_cursor(f, chunks[2], state);
    }
}

fn draw_peers(f: &mut Frame, area: Rect, state: &UiState) {
    let mut items = Vec::new();
    let local_style = Style::default().fg(Color::Green);
    items.push(ListItem::new(Line::from(Span::styled(
        format!("{} (local)", state.local_display),
        local_style,
    ))));
    for peer in state.peers.values() {
        if peer.display == state.local_display {
            continue;
        }
        let speaking = peer
            .last_voice
            .map(|t| t.elapsed() < Duration::from_millis(600))
            .unwrap_or(false);
        let line = format!(
            "{} {} peer",
            peer.display,
            if speaking { "●" } else { " " }
        );
        items.push(ListItem::new(Line::from(Span::styled(
            line,
            Style::default().fg(Color::Blue),
        ))));
    }
    let block = Block::default().title("Peers").borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_chat(f: &mut Frame, area: Rect, state: &UiState) {
    let mut lines = Vec::new();
    for line in state.chat.iter() {
        let ts = format_time(line.timestamp);
        let name_style = if line.name == state.user_name {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("[{}] ", ts), Style::default().add_modifier(Modifier::DIM)),
            Span::styled(format!("{}: ", line.name), name_style),
            Span::raw(line.text.clone()),
        ]));
    }
    let block = Block::default().title("Chat").borders(Borders::ALL);
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_status(f: &mut Frame, area: Rect, state: &UiState) {
    let peers = state
        .peers
        .keys()
        .filter(|peer_id| **peer_id != state.local_peer_id)
        .count();
    let total = peers + 1;
    let mic = if state.ptt_enabled {
        if state.mic_active { "mic: on" } else { "mic: off" }
    } else {
        "mic: open"
    };
    let ptt_label = if state.ptt_enabled {
        format!("ptt: {}", state.ptt_key.label())
    } else {
        "ptt: off".to_string()
    };
    let rx_on = pulse_active(state.last_rx, Duration::from_millis(300));
    let tx_on = pulse_active(state.last_tx, Duration::from_millis(300));
    let call_info = if let Some((_, from)) = state.incoming_call {
        format!("incoming {}", short_peer(&from))
    } else if let Some((_, peer)) = state.pending_call {
        format!("dialing {}", short_peer(&peer))
    } else if state.active_session != state.channel_session {
        let count = if state.active_call_peer.is_some() { 2 } else { 1 };
        if let Some(peer) = state.active_call_peer {
            format!("active {} [{}]", short_peer(&peer), count)
        } else {
            format!("active [{}]", count)
        }
    } else {
        format!("channel [{}]", total)
    };
    let mut spans = vec![
        Span::styled("channel: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{} [{}]", state.channel, total),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled("peers: ", Style::default().fg(Color::Cyan)),
        Span::styled(format!("{}", peers), Style::default().fg(Color::White)),
        Span::raw(" | "),
        Span::styled("mic: ", Style::default().fg(Color::Cyan)),
        Span::styled(mic, Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled("ptt: ", Style::default().fg(Color::Cyan)),
        Span::styled(ptt_label, Style::default().fg(Color::Yellow)),
        Span::raw(" | "),
        Span::styled("mute: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            if state.muted { "on" } else { "off" },
            Style::default().fg(if state.muted { Color::Red } else { Color::Green }),
        ),
        Span::raw(" | "),
        Span::styled("quality: ", Style::default().fg(Color::Cyan)),
        Span::styled(state.audio_quality.clone(), Style::default().fg(Color::White)),
        Span::raw(" | "),
        Span::styled("theme: ", Style::default().fg(Color::Cyan)),
        Span::styled(state.theme.clone(), Style::default().fg(Color::White)),
        Span::raw(" | "),
        Span::styled("p2p: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            if state.prefer_p2p { "prefer" } else { "any" },
            Style::default().fg(Color::White),
        ),
        Span::raw(" | "),
        Span::styled("RX ", Style::default().fg(Color::Cyan)),
        Span::styled(
            "●",
            Style::default().fg(if rx_on { Color::Green } else { Color::DarkGray }),
        ),
        Span::raw(" "),
        Span::styled("TX ", Style::default().fg(Color::Cyan)),
        Span::styled(
            "●",
            Style::default().fg(if tx_on { Color::Red } else { Color::DarkGray }),
        ),
        Span::raw(" | "),
        Span::styled("call: ", Style::default().fg(Color::Cyan)),
        Span::styled(call_info, Style::default().fg(Color::White)),
        Span::raw(" | "),
        Span::styled("keys: ", Style::default().fg(Color::Magenta)),
        Span::styled("ctrl+q", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" quit "),
        Span::styled("ctrl+a", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" quality "),
        Span::styled("m", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" mute "),
        Span::styled("tab", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" focus "),
        Span::styled("enter", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" send "),
        Span::styled(
            state.ptt_key.label(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" talk"),
    ];
    if state.incoming_call.is_some() {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            "a",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" accept "));
        spans.push(Span::styled(
            "d",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" decline"));
    }
    let header = Line::from(spans);
    let block = Block::default().borders(Borders::TOP);
    let paragraph = Paragraph::new(header).block(block);
    f.render_widget(paragraph, area);
}

fn draw_input(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .title(if state.focus == Focus::Input {
            "Input"
        } else {
            "Input (TAB to focus)"
        })
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(format!("> {}", state.input))
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn set_input_cursor(f: &mut Frame, area: Rect, state: &UiState) {
    let inner = area.inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let x = inner.x + 2 + state.input.len() as u16;
    let y = inner.y;
    f.set_cursor(x.min(inner.right().saturating_sub(1)), y);
}

fn pulse_active(last: Option<Instant>, window: Duration) -> bool {
    last.map(|t| t.elapsed() <= window).unwrap_or(false)
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

fn build_sdk_config(
    user_cfg: &UserConfig,
    port: u16,
    relay: bool,
    voice: bool,
    invite: Option<String>,
) -> RiftConfig {
    RiftConfig {
        identity_path: None,
        listen_port: port,
        relay,
        user_name: user_cfg.user.name.clone(),
        audio: AudioConfigSdk {
            enabled: voice,
            input_device: user_cfg.audio.input_device.clone(),
            output_device: user_cfg.audio.output_device.clone(),
            quality: user_cfg.audio.quality.clone().unwrap_or_else(|| "medium".to_string()),
            ptt: user_cfg.audio.ptt.unwrap_or(false),
            vad: user_cfg.audio.vad.unwrap_or(true),
            mute_output: user_cfg.audio.mute_output.unwrap_or(false),
            emit_voice_frames: false,
        },
        network: NetworkConfigSdk {
            prefer_p2p: user_cfg.network.prefer_p2p.unwrap_or(true),
            local_ports: user_cfg.network.local_ports.clone(),
            known_peers: Vec::new(),
            invite,
        },
    }
}

fn short_peer(peer: &rift_core::PeerId) -> String {
    let hex = peer.to_hex();
    let short = &hex[..8];
    short.to_string()
}

fn parse_session_id(input: &str) -> Result<RiftSessionId> {
    let trimmed = input.trim().trim_start_matches("0x");
    let bytes = hex::decode(trimmed).context("invalid session hex")?;
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!("session id must be 32 bytes"));
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&bytes);
    Ok(RiftSessionId(raw))
}

fn resolve_peer_input(state: &UiState, input: &str) -> Option<rift_core::PeerId> {
    let target = input.trim();
    if target.is_empty() {
        return None;
    }
    if target.eq_ignore_ascii_case("me") {
        return Some(state.local_peer_id);
    }
    let needle = target.to_lowercase();
    for peer_id in state.peers.keys() {
        let short = short_peer(peer_id);
        if short.eq_ignore_ascii_case(&needle) {
            return Some(*peer_id);
        }
    }
    None
}

fn format_time(ts: u64) -> String {
    let secs = ts / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
