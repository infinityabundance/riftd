# Rift Android Client

This document covers the Kotlin/Compose client under `android/` and the JNI
bridge to `rift-sdk`.

## Project layout

- `android/app` – Compose UI + app entry point.
- `android/rift-sdk-android` – Kotlin wrapper around JNI bindings.
- `crates/rift-sdk/src/android_jni.rs` – JNI surface used by Android.

## Build (debug)

```bash
cd android
./build-rust-android.sh
./gradlew assembleDebug
```

The APK will be at:

```
android/app/build/outputs/apk/debug/app-debug.apk
```

## Install to emulator/device

```bash
/opt/android-sdk/platform-tools/adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

## Usage

- Home: enter a channel or invite link, then Join.
- Call: chat + peers + PTT.
- Settings: audio quality and TURN server overrides (applies to the next join).

## Screenshot (text description)

- Home screen shows channel fields, invite input, DHT toggles, and join buttons.
- Call screen shows peer list on the left, chat on the right, and a large PTT bar.
- Settings screen exposes audio quality and TURN server inputs.

## JNI notes

The Kotlin wrapper calls into these JNI methods:

- `init`, `joinChannel`, `leaveChannel`, `sendChat`
- `startPtt`, `stopPtt`, `pollEvent`
- `setDhtEnabled`, `setBootstrapNodes`, `setInvite`
- `setTurnServers`, `setAudioQuality`, `generateInvite`

JNI entry points live in `crates/rift-sdk/src/android_jni.rs`.
