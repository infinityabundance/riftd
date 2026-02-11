//! JNI bindings for Android integration.
//!
//! This module exposes a small JNI surface for initializing the SDK and
//! bridging events into the Java/Kotlin layer.

use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jobject};
use jni::JNIEnv;
use tokio::runtime::Runtime;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Once;
use ndk_context::initialize_android_context;

use crate::{RiftConfig, RiftEvent, RiftHandle};

/// Bundle of runtime + handle kept behind a raw pointer for JNI.
struct JniHandle {
    handle: RiftHandle,
    rt: Runtime,
}

/// Last error string for JNI callers to inspect.
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
/// Global panic hook to surface crashes to Android logs.
static PANIC_HOOK: Once = Once::new();

fn set_last_error(message: &str) {
    if let Ok(mut guard) = LAST_ERROR.lock() {
        *guard = Some(message.to_string());
    }
}

/// Write an error to both the in-memory last-error slot and Android logs.
fn log_error(message: &str) {
    set_last_error(message);
    const ANDROID_LOG_ERROR: i32 = 6;
    unsafe {
        let tag = CString::new("RiftSdk").unwrap_or_default();
        let fmt = CString::new("%s").unwrap_or_default();
        let msg = CString::new(message).unwrap_or_default();
        #[allow(improper_ctypes)]
        extern "C" {
            fn __android_log_print(prio: i32, tag: *const i8, fmt: *const i8, ...) -> i32;
        }
        __android_log_print(
            ANDROID_LOG_ERROR,
            tag.as_ptr() as *const i8,
            fmt.as_ptr() as *const i8,
            msg.as_ptr() as *const i8,
        );
    }
}

/// Convert a raw JNI handle into a mutable reference.
fn with_handle<'a>(handle: jlong) -> &'a mut JniHandle {
    unsafe { &mut *(handle as *mut JniHandle) }
}

/// Initialize the SDK from Android and return a native handle pointer.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_init(
    mut env: JNIEnv,
    _class: JClass,
    context: JObject,
    config_path: JString,
) -> jlong {
    if !context.is_null() {
        if let Ok(vm) = env.get_java_vm() {
            unsafe {
                initialize_android_context(vm.get_java_vm_pointer() as *mut _, context.into_raw() as *mut _);
            }
        } else {
            log_error("failed to get JavaVM for ndk-context");
        }
    } else {
        log_error("android context was null");
    }
    PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let mut message = String::new();
            if let Some(location) = info.location() {
                message.push_str(&format!("panic at {}:{} ", location.file(), location.line()));
            }
            if let Some(payload) = info.payload().downcast_ref::<&str>() {
                message.push_str(payload);
            } else if let Some(payload) = info.payload().downcast_ref::<String>() {
                message.push_str(payload);
            } else {
                message.push_str("unknown panic");
            }
            log_error(&message);
        }));
    });
    let config_path: Option<String> = if config_path.is_null() {
        None
    } else {
        env.get_string(&config_path).ok().map(|s| s.to_string_lossy().to_string())
    };

    let mut config = RiftConfig::default();
    if let Some(path) = config_path {
        let base_path = PathBuf::from(path);
        let (config_dir, config_file) = if base_path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let dir = base_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
            (dir, base_path)
        } else {
            let file = base_path.join("config.toml");
            (base_path, file)
        };
        let _ = std::fs::create_dir_all(&config_dir);
        if let Ok(content) = std::fs::read_to_string(&config_file) {
            if let Ok(parsed) = toml::from_str::<RiftConfig>(&content) {
                config = parsed;
            }
        }
        if config.identity_path.is_none() {
            config.identity_path = Some(config_dir.join("identity.key"));
        }
        if config.security.known_hosts_path.is_none() {
            config.security.known_hosts_path = Some(config_dir.join("known_hosts"));
        }
        if config.security.audit_log_path.is_none() {
            config.security.audit_log_path = Some(config_dir.join("audit.log"));
        }
    }
    config.audio.enabled = true;
    config.audio.ptt = true;
    config.audio.vad = false;
    config.audio.allow_fail = true;
    config.dht.enabled = true;
    config.listen_port = 0;

    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            log_error(&format!("tokio runtime init failed: {err}"));
            return 0;
        }
    };
    let handle = match rt.block_on(RiftHandle::new(config)) {
        Ok(handle) => handle,
        Err(err) => {
            log_error(&format!("RiftHandle::new failed: {err}"));
            return 0;
        }
    };

    let boxed = Box::new(JniHandle { handle, rt });
    Box::into_raw(boxed) as jlong
}

/// Join a channel from the Android layer.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_joinChannel(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    name: JString,
    password: JString,
    internet: jboolean,
    dht: jboolean,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let name = env.get_string(&name).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let password = if password.is_null() {
        None
    } else {
        env.get_string(&password).ok().map(|s| s.to_string_lossy().to_string())
    };
    let internet = internet != 0;
    let dht = dht != 0;
    let handle = with_handle(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        handle.rt.block_on(handle.handle.join_channel(&name, password.as_deref(), internet || dht))
    }));
    match result {
        Ok(Ok(_)) => 0,
        Ok(Err(err)) => {
            log_error(&format!("join_channel failed: {err}"));
            -1
        }
        Err(_) => {
            log_error("join_channel panicked");
            -1
        }
    }
}

/// Set DHT bootstrap nodes.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setBootstrapNodes(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    nodes: JString,
) {
    if handle == 0 {
        return;
    }
    let nodes = env
        .get_string(&nodes)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let list = nodes
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let handle = with_handle(handle);
    let _ = handle.rt.block_on(handle.handle.set_bootstrap_nodes(list));
}

/// Enable or disable DHT discovery.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setDhtEnabled(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    enabled: jboolean,
) {
    if handle == 0 {
        return;
    }
    let handle = with_handle(handle);
    let _ = handle
        .rt
        .block_on(handle.handle.set_dht_enabled(enabled != 0));
}

/// Override the invite link used for joining.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setInvite(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    invite: JString,
) {
    if handle == 0 {
        return;
    }
    let invite = if invite.is_null() {
        None
    } else {
        env.get_string(&invite)
            .ok()
            .map(|s| s.to_string_lossy().to_string())
    };
    let handle = with_handle(handle);
    let _ = handle.rt.block_on(handle.handle.set_invite(invite));
}

/// Configure TURN servers for relay fallback.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setTurnServers(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    servers: JString,
) {
    if handle == 0 {
        return;
    }
    let servers = env
        .get_string(&servers)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let list = servers
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let handle = with_handle(handle);
    let _ = handle.rt.block_on(handle.handle.set_turn_servers(list));
}

/// Override the audio quality preset.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setAudioQuality(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    quality: JString,
) {
    if handle == 0 {
        return;
    }
    let quality = if quality.is_null() {
        None
    } else {
        env.get_string(&quality)
            .ok()
            .map(|s| s.to_string_lossy().to_string())
    };
    let handle = with_handle(handle);
    let _ = handle.rt.block_on(handle.handle.set_audio_quality(quality));
}

/// Generate an invite string for sharing.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_generateInvite(
    mut env: JNIEnv,
    _class: JClass,
    channel: JString,
    password: JString,
    known_peers: JString,
) -> jobject {
    let channel = env
        .get_string(&channel)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if channel.is_empty() {
        return std::ptr::null_mut();
    }
    let password = if password.is_null() {
        None
    } else {
        env.get_string(&password).ok().map(|s| s.to_string_lossy().to_string())
    };
    let peers = env
        .get_string(&known_peers)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let known_peers = peers
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect::<Vec<_>>();
    let invite = rift_core::generate_invite(&channel, password.as_deref(), known_peers, Vec::new());
    let encoded = rift_core::encode_invite(&invite);
    let Ok(jstr) = env.new_string(encoded) else {
        return std::ptr::null_mut();
    };
    JObject::from(jstr).into_raw()
}

/// Leave the current channel.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_leaveChannel(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    name: JString,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let name = env.get_string(&name).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let handle = with_handle(handle);
    let result = handle.rt.block_on(handle.handle.leave_channel(&name));
    if result.is_ok() { 0 } else { -1 }
}

/// Send a chat message to peers.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_sendChat(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    text: JString,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let text = env.get_string(&text).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let handle = with_handle(handle);
    let result = handle.rt.block_on(handle.handle.send_chat(&text));
    if result.is_ok() { 0 } else { -1 }
}

/// Enable push-to-talk.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_startPtt(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let handle = with_handle(handle);
    handle.handle.set_ptt_active(true);
    0
}

/// Disable push-to-talk.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_stopPtt(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let handle = with_handle(handle);
    handle.handle.set_ptt_active(false);
    0
}

/// Set mute state for microphone.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_setMute(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    muted: jboolean,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let handle = with_handle(handle);
    handle.handle.set_mute(muted != 0);
    0
}

/// Start a call to a specific peer.
/// Returns the session ID as a hex string, or null on failure.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_startCall(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    peer_hex: JString,
) -> jobject {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    let peer_hex = env
        .get_string(&peer_hex)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let peer_bytes = match hex::decode(&peer_hex) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            log_error("invalid peer id hex");
            return std::ptr::null_mut();
        }
    };
    let peer_id = rift_core::PeerId(peer_bytes);
    let handle = with_handle(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        handle.rt.block_on(handle.handle.start_call(peer_id))
    }));
    match result {
        Ok(Ok(session)) => {
            let session_hex = hex::encode(session.0);
            match env.new_string(session_hex) {
                Ok(jstr) => JObject::from(jstr).into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
        Ok(Err(err)) => {
            log_error(&format!("start_call failed: {err}"));
            std::ptr::null_mut()
        }
        Err(_) => {
            log_error("start_call panicked");
            std::ptr::null_mut()
        }
    }
}

/// Accept an incoming call.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_acceptCall(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    session_hex: JString,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let session_hex = env
        .get_string(&session_hex)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let session_bytes = match hex::decode(&session_hex) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            log_error("invalid session id hex");
            return -1;
        }
    };
    let session = rift_protocol::SessionId(session_bytes);
    let handle = with_handle(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        handle.rt.block_on(handle.handle.accept_call(session))
    }));
    match result {
        Ok(Ok(_)) => 0,
        Ok(Err(err)) => {
            log_error(&format!("accept_call failed: {err}"));
            -1
        }
        Err(_) => {
            log_error("accept_call panicked");
            -1
        }
    }
}

/// Decline an incoming call with optional reason.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_declineCall(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    session_hex: JString,
    reason: JString,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let session_hex = env
        .get_string(&session_hex)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let session_bytes = match hex::decode(&session_hex) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            log_error("invalid session id hex");
            return -1;
        }
    };
    let reason = if reason.is_null() {
        None
    } else {
        env.get_string(&reason)
            .ok()
            .map(|s| s.to_string_lossy().to_string())
    };
    let session = rift_protocol::SessionId(session_bytes);
    let handle = with_handle(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        handle.rt.block_on(handle.handle.decline_call(session, reason.as_deref()))
    }));
    match result {
        Ok(Ok(_)) => 0,
        Ok(Err(err)) => {
            log_error(&format!("decline_call failed: {err}"));
            -1
        }
        Err(_) => {
            log_error("decline_call panicked");
            -1
        }
    }
}

/// End an active call.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_endCall(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    session_hex: JString,
) -> jint {
    if handle == 0 {
        return -1;
    }
    let session_hex = env
        .get_string(&session_hex)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let session_bytes = match hex::decode(&session_hex) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            log_error("invalid session id hex");
            return -1;
        }
    };
    let session = rift_protocol::SessionId(session_bytes);
    let handle = with_handle(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        handle.rt.block_on(handle.handle.end_call(session))
    }));
    match result {
        Ok(Ok(_)) => 0,
        Ok(Err(err)) => {
            log_error(&format!("end_call failed: {err}"));
            -1
        }
        Err(_) => {
            log_error("end_call panicked");
            -1
        }
    }
}

/// Poll the next event and return it as a Java object.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_pollEvent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobject {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    let handle = with_handle(handle);
    let event = handle.handle.try_next_event();
    let Some(event) = event else { return std::ptr::null_mut(); };

    let class = env.find_class("com/example/riftmobile/sdk/RiftEventDto").ok();
    let Some(class) = class else { return std::ptr::null_mut(); };

    let (type_str, from, text, peers, status) = match event {
        RiftEvent::IncomingChat(chat) => (
            "chat".to_string(),
            Some(chat.from.to_hex()),
            Some(chat.text),
            None,
            Some(format!("{}", chat.timestamp)),
        ),
        RiftEvent::PeerJoinedChannel { peer, .. } => (
            "peer_joined".to_string(),
            Some(peer.to_hex()),
            None,
            None,
            None,
        ),
        RiftEvent::PeerLeftChannel { peer, .. } => (
            "peer_left".to_string(),
            Some(peer.to_hex()),
            None,
            None,
            None,
        ),
        RiftEvent::IncomingCall { session, from, rndzv_srt_uri } => (
            "incoming_call".to_string(),
            Some(from.to_hex()),
            rndzv_srt_uri, // Pass SRT URI as text if present
            None,
            Some(hex::encode(session.0)), // Session ID in status field
        ),
        RiftEvent::CallStateChanged { session, state } => (
            "call_state".to_string(),
            None,
            Some(format!("{:?}", state)), // Call state as text
            None,
            Some(hex::encode(session.0)), // Session ID in status field
        ),
        RiftEvent::AudioLevel { peer, level } => (
            "audio_level".to_string(),
            Some(peer.to_hex()),
            Some(format!("{:.3}", level)), // Level as text
            None,
            None,
        ),
        RiftEvent::StatsUpdate { global, .. } => (
            "stats".to_string(),
            None,
            None,
            Some(global.num_peers as i32),
            None,
        ),
        RiftEvent::SecurityNotice { message } => (
            "security".to_string(),
            None,
            Some(message),
            None,
            None,
        ),
        RiftEvent::PeerFingerprint { peer, fingerprint } => (
            "fingerprint".to_string(),
            Some(peer.to_hex()),
            Some(fingerprint),
            None,
            None,
        ),
        _ => ("other".to_string(), None, None, None, None),
    };

    let type_j = env.new_string(type_str).unwrap();
    let from_j = from.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let text_j = text.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let status_j = status.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let peers_obj = peers.map(|v| {
        let integer_class = env.find_class("java/lang/Integer").unwrap();
        env.new_object(integer_class, "(I)V", &[JValue::Int(v)]).unwrap()
    }).unwrap_or(JObject::null());

    let obj = env.new_object(
        class,
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Integer;Ljava/lang/String;)V",
        &[
            JValue::Object(&JObject::from(type_j)),
            JValue::Object(&from_j),
            JValue::Object(&text_j),
            JValue::Object(&peers_obj),
            JValue::Object(&status_j),
        ],
    ).ok();
    obj.map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Return the last error string stored by JNI helpers.
#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_sdk_RiftNative_lastError(
    env: JNIEnv,
    _class: JClass,
) -> jobject {
    let msg = LAST_ERROR.lock().ok().and_then(|g| g.clone());
    let Some(msg) = msg else { return std::ptr::null_mut(); };
    let Ok(jstr) = env.new_string(msg) else { return std::ptr::null_mut(); };
    JObject::from(jstr).into_raw()
}
