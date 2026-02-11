//! C FFI bindings for the Rift SDK.
//!
//! This module exposes a minimal C ABI for initializing the SDK, joining
//! channels, and receiving events. It is intentionally narrow to keep ABI
//! compatibility manageable.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use crate::{RiftConfig, RiftEvent, RiftHandle, RiftError, SDK_VERSION, SDK_ABI_VERSION};
use rift_protocol::SessionId;
use rift_core::PeerId;

#[repr(C)]
pub struct RiftHandleC {
    /// Dedicated Tokio runtime for the SDK.
    runtime: tokio::runtime::Runtime,
    /// Rust-side SDK handle.
    handle: RiftHandle,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PeerIdC {
    /// Raw peer id bytes.
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SessionIdC {
    /// Raw session id bytes.
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum RiftEventTag {
    None = 0,
    IncomingChat = 1,
    IncomingCall = 2,
    CallStateChanged = 3,
    PeerJoined = 4,
    PeerLeft = 5,
    AudioLevel = 6,
}

#[repr(C)]
pub struct RiftEventC {
    /// Event type discriminator.
    pub tag: RiftEventTag,
    /// Peer associated with the event.
    pub peer: PeerIdC,
    /// Session associated with the event.
    pub session: SessionIdC,
    /// Audio level (if applicable).
    pub level: f32,
    /// Optional text payload (heap-allocated C string).
    pub text: *mut c_char,
}

#[repr(C)]
pub enum RiftErrorCode {
    Ok = 0,
    InvalidConfig = 1,
    InitFailed = 2,
    NotJoined = 3,
    Other = 255,
}

fn peer_to_c(peer: PeerId) -> PeerIdC {
    PeerIdC { bytes: peer.0 }
}

fn session_to_c(session: SessionId) -> SessionIdC {
    SessionIdC { bytes: session.0 }
}

/// Initialize the SDK from a TOML config path (or defaults if null).
///
/// # Safety
/// `config_path` must be a valid null-terminated string if non-null.
#[no_mangle]
pub extern "C" fn rift_init(config_path: *const c_char, out_error: *mut RiftErrorCode) -> *mut RiftHandleC {
    unsafe {
        if !out_error.is_null() {
            *out_error = RiftErrorCode::Ok;
        }
    }

    let config = if config_path.is_null() {
        // Use port 0 (OS-assigned) by default for FFI clients to avoid conflicts
        let mut cfg = RiftConfig::default();
        cfg.listen_port = 0;
        cfg
    } else {
        let c_str = unsafe { CStr::from_ptr(config_path) };
        match c_str.to_str() {
            Ok(path) => {
                match std::fs::read_to_string(path) {
                    Ok(content) => match toml::from_str::<RiftConfig>(&content) {
                        Ok(cfg) => cfg,
                        Err(_) => {
                            unsafe {
                                if !out_error.is_null() {
                                    *out_error = RiftErrorCode::InvalidConfig;
                                }
                            }
                            return ptr::null_mut();
                        }
                    },
                    Err(_) => {
                        unsafe {
                            if !out_error.is_null() {
                                *out_error = RiftErrorCode::InvalidConfig;
                            }
                        }
                        return ptr::null_mut();
                    }
                }
            }
            Err(_) => {
                unsafe {
                    if !out_error.is_null() {
                        *out_error = RiftErrorCode::InvalidConfig;
                    }
                }
                return ptr::null_mut();
            }
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(_) => {
            unsafe {
                if !out_error.is_null() {
                    *out_error = RiftErrorCode::InitFailed;
                }
            }
            return ptr::null_mut();
        }
    };

    let handle = match runtime.block_on(RiftHandle::new(config)) {
        Ok(handle) => handle,
        Err(_) => {
            unsafe {
                if !out_error.is_null() {
                    *out_error = RiftErrorCode::InitFailed;
                }
            }
            return ptr::null_mut();
        }
    };

    let boxed = Box::new(RiftHandleC { runtime, handle });
    Box::into_raw(boxed)
}

/// Return the SDK version string.
#[no_mangle]
pub extern "C" fn rift_sdk_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Return the SDK ABI version.
#[no_mangle]
pub extern "C" fn rift_sdk_abi_version() -> c_int {
    SDK_ABI_VERSION as c_int
}

/// Free a previously allocated Rift handle.
///
/// # Safety
/// `handle` must be a pointer returned by `rift_init`.
#[no_mangle]
pub extern "C" fn rift_free(handle: *mut RiftHandleC) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

/// Join a channel by name/password.
///
/// # Safety
/// `handle` must be valid. Strings must be null-terminated if non-null.
#[no_mangle]
pub extern "C" fn rift_join_channel(
    handle: *mut RiftHandleC,
    name: *const c_char,
    password: *const c_char,
    internet: c_int,
) -> c_int {
    if handle.is_null() || name.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_string();
    let password = if password.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(password) }.to_string_lossy().to_string())
    };
    let internet = internet != 0;
    let result = handle.runtime.block_on(handle.handle.join_channel(&name, password.as_deref(), internet));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Leave the current channel.
///
/// # Safety
/// `handle` must be valid. `name` must be a null-terminated string if non-null.
#[no_mangle]
pub extern "C" fn rift_leave_channel(handle: *mut RiftHandleC, name: *const c_char) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let name = if name.is_null() {
        "".to_string()
    } else {
        unsafe { CStr::from_ptr(name) }.to_string_lossy().to_string()
    };
    let result = handle.runtime.block_on(handle.handle.leave_channel(&name));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Get the invite string for the current channel.
///
/// Returns a newly allocated string that must be freed with `rift_free_string`.
/// Returns null if not joined or no invite available.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_get_invite(handle: *mut RiftHandleC) -> *mut c_char {
    if handle.is_null() {
        return ptr::null_mut();
    }
    let handle = unsafe { &mut *handle };
    let invite = handle.runtime.block_on(handle.handle.get_invite());
    match invite {
        Some(s) => {
            match CString::new(s) {
                Ok(cs) => cs.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
        None => ptr::null_mut(),
    }
}

/// Free a string returned by `rift_get_invite`.
///
/// # Safety
/// `s` must be a string returned by `rift_get_invite` or null.
#[no_mangle]
pub extern "C" fn rift_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Send a chat message to peers.
///
/// # Safety
/// `handle` must be valid. `text` must be a null-terminated string if non-null.
#[no_mangle]
pub extern "C" fn rift_send_chat(handle: *mut RiftHandleC, text: *const c_char) -> c_int {
    if handle.is_null() || text.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().to_string();
    let result = handle.runtime.block_on(handle.handle.send_chat(&text));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Enable push-to-talk.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_start_ptt(handle: *mut RiftHandleC) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    handle.handle.set_ptt_active(true);
    0
}

/// Disable push-to-talk.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_stop_ptt(handle: *mut RiftHandleC) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    handle.handle.set_ptt_active(false);
    0
}

/// Mute or unmute microphone capture.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_set_mute(handle: *mut RiftHandleC, muted: c_int) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    handle.handle.set_mute(muted != 0);
    0
}

/// Start a call to a specific peer.
///
/// # Safety
/// `handle` must be valid. `peer` must point to a valid `PeerIdC`.
#[no_mangle]
pub extern "C" fn rift_start_call(handle: *mut RiftHandleC, peer: *const PeerIdC) -> SessionIdC {
    if handle.is_null() || peer.is_null() {
        return SessionIdC { bytes: [0u8; 32] };
    }
    let handle = unsafe { &mut *handle };
    let peer = unsafe { &*peer };
    let peer_id = PeerId(peer.bytes);
    let result = handle.runtime.block_on(handle.handle.start_call(peer_id));
    match result {
        Ok(session) => session_to_c(session),
        Err(_) => SessionIdC { bytes: [0u8; 32] },
    }
}

/// Accept an incoming call.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_accept_call(handle: *mut RiftHandleC, session: SessionIdC) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let session = SessionId(session.bytes);
    let result = handle.runtime.block_on(handle.handle.accept_call(session));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Decline an incoming call with optional reason.
///
/// # Safety
/// `handle` must be valid. `reason` must be a null-terminated string if non-null.
#[no_mangle]
pub extern "C" fn rift_decline_call(
    handle: *mut RiftHandleC,
    session: SessionIdC,
    reason: *const c_char,
) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let session = SessionId(session.bytes);
    let reason = if reason.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(reason) }.to_string_lossy().to_string())
    };
    let result = handle
        .runtime
        .block_on(handle.handle.decline_call(session, reason.as_deref()));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// End an active call.
///
/// # Safety
/// `handle` must be valid.
#[no_mangle]
pub extern "C" fn rift_end_call(handle: *mut RiftHandleC, session: SessionIdC) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let session = SessionId(session.bytes);
    let result = handle.runtime.block_on(handle.handle.end_call(session));
    match result {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Fetch the next event without blocking.
///
/// # Safety
/// `handle` and `out_event` must be valid pointers.
#[no_mangle]
pub extern "C" fn rift_next_event(handle: *mut RiftHandleC, out_event: *mut RiftEventC) -> c_int {
    if handle.is_null() || out_event.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let event = handle.handle.try_next_event();
    unsafe {
        (*out_event) = RiftEventC {
            tag: RiftEventTag::None,
            peer: PeerIdC { bytes: [0u8; 32] },
            session: SessionIdC { bytes: [0u8; 32] },
            level: 0.0,
            text: ptr::null_mut(),
        };
    }
    let Some(event) = event else { return 0; };

    match event {
        RiftEvent::IncomingChat(chat) => {
            let text = CString::new(chat.text).unwrap_or_default().into_raw();
            unsafe {
                (*out_event).tag = RiftEventTag::IncomingChat;
                (*out_event).peer = peer_to_c(chat.from);
                (*out_event).text = text;
            }
        }
        RiftEvent::IncomingCall {
            session,
            from,
            rndzv_srt_uri: _,
        } => unsafe {
            (*out_event).tag = RiftEventTag::IncomingCall;
            (*out_event).peer = peer_to_c(from);
            (*out_event).session = session_to_c(session);
        },
        RiftEvent::CallStateChanged { session, .. } => unsafe {
            (*out_event).tag = RiftEventTag::CallStateChanged;
            (*out_event).session = session_to_c(session);
        },
        RiftEvent::PeerJoinedChannel { peer, .. } => unsafe {
            (*out_event).tag = RiftEventTag::PeerJoined;
            (*out_event).peer = peer_to_c(peer);
        },
        RiftEvent::PeerLeftChannel { peer, .. } => unsafe {
            (*out_event).tag = RiftEventTag::PeerLeft;
            (*out_event).peer = peer_to_c(peer);
        },
        RiftEvent::AudioLevel { peer, level } => unsafe {
            (*out_event).tag = RiftEventTag::AudioLevel;
            (*out_event).peer = peer_to_c(peer);
            (*out_event).level = level;
        },
        RiftEvent::CodecSelected { .. }
        | RiftEvent::PeerCapabilities { .. }
        | RiftEvent::AudioBitrate { .. }
        | RiftEvent::StatsUpdate { .. }
        | RiftEvent::RouteUpdated { .. }
        | RiftEvent::GroupTopology { .. }
        | RiftEvent::PeerFingerprint { .. }
        | RiftEvent::SecurityNotice { .. }
        | RiftEvent::VoiceFrame { .. } => {}
    }
    1
}

/// Free strings owned by an event.
///
/// # Safety
/// `event` must be a pointer returned by `rift_next_event`.
#[no_mangle]
pub extern "C" fn rift_event_free(event: *mut RiftEventC) {
    if event.is_null() {
        return;
    }
    unsafe {
        if !(*event).text.is_null() {
            drop(CString::from_raw((*event).text));
            (*event).text = ptr::null_mut();
        }
    }
}
