use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use crate::{RiftConfig, RiftEvent, RiftHandle, RiftError};
use rift_protocol::SessionId;
use rift_core::PeerId;

#[repr(C)]
pub struct RiftHandleC {
    runtime: tokio::runtime::Runtime,
    handle: RiftHandle,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PeerIdC {
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SessionIdC {
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
    pub tag: RiftEventTag,
    pub peer: PeerIdC,
    pub session: SessionIdC,
    pub level: f32,
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

#[no_mangle]
pub extern "C" fn rift_init(config_path: *const c_char, out_error: *mut RiftErrorCode) -> *mut RiftHandleC {
    unsafe {
        if !out_error.is_null() {
            *out_error = RiftErrorCode::Ok;
        }
    }

    let config = if config_path.is_null() {
        RiftConfig::default()
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

#[no_mangle]
pub extern "C" fn rift_free(handle: *mut RiftHandleC) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

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
        RiftEvent::IncomingCall { session, from } => unsafe {
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
        | RiftEvent::VoiceFrame { .. } => {}
    }
    1
}

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
