# Rift Mobile (Android)

This is a minimal developer Android client for Rift (text + voice PTT).

## Build Steps

1. Install Rust Android target and cargo-ndk:

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
```

2. Build `librift_sdk.so` into `jniLibs` (arm64 + x86_64 for emulator):

```bash
cd android
./build-rust-android.sh
```

3. Build the APK from Android Studio or CLI (NDK 26.1 recommended):

```bash
cd android
./gradlew assembleDebug
```

## Notes

- The app uses JNI to call into `rift-sdk`.
- Press and hold the PTT button to transmit audio.
- The app assumes a single active channel/session.
