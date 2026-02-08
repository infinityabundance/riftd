#![cfg(target_os = "android")]

use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jobject};
use jni::JNIEnv;
use tokio::runtime::Runtime;

use crate::{RiftConfig, RiftEvent, RiftHandle};

struct JniHandle {
    handle: RiftHandle,
    rt: Runtime,
}

fn with_handle<'a>(handle: jlong) -> &'a mut JniHandle {
    unsafe { &mut *(handle as *mut JniHandle) }
}

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_init(
    mut env: JNIEnv,
    _class: JClass,
    config_path: JString,
) -> jlong {
    let config_path: Option<String> = if config_path.is_null() {
        None
    } else {
        env.get_string(&config_path).ok().map(|s| s.to_string_lossy().to_string())
    };

    let mut config = if let Some(path) = config_path {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        toml::from_str::<RiftConfig>(&content).unwrap_or_default()
    } else {
        RiftConfig::default()
    };
    config.audio.enabled = true;
    config.audio.ptt = true;
    config.audio.vad = false;
    config.dht.enabled = true;

    let rt = Runtime::new().unwrap();
    let handle = match rt.block_on(RiftHandle::new(config)) {
        Ok(handle) => handle,
        Err(_) => return 0,
    };

    let boxed = Box::new(JniHandle { handle, rt });
    Box::into_raw(boxed) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_joinChannel(
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
    let result = handle.rt.block_on(handle.handle.join_channel(&name, password.as_deref(), internet || dht));
    if result.is_ok() { 0 } else { -1 }
}

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_leaveChannel(
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

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_sendChat(
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

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_startPtt(
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

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_stopPtt(
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

#[no_mangle]
pub extern "system" fn Java_com_example_riftmobile_RiftNative_pollEvent(
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

    let class = env.find_class("com/example/riftmobile/RiftEventDto").ok();
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
        _ => ("other".to_string(), None, None, None, None),
    };

    let ctor = env.get_method_id(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Integer;Ljava/lang/String;)V");
    let Ok(ctor) = ctor else { return std::ptr::null_mut(); };

    let type_j = env.new_string(type_str).unwrap();
    let from_j = from.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let text_j = text.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let status_j = status.map(|v| env.new_string(v).unwrap()).map(JObject::from).unwrap_or(JObject::null());
    let peers_obj = peers.map(|v| {
        let integer_class = env.find_class("java/lang/Integer").unwrap();
        let integer_ctor = env.get_method_id(integer_class, "<init>", "(I)V").unwrap();
        env.new_object(integer_class, integer_ctor, &[JValue::Int(v)]).unwrap()
    }).unwrap_or(JObject::null());

    let obj = env.new_object(
        class,
        ctor,
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
