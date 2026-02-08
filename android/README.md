# Rift Mobile (Android)

This is a minimal developer Android client for Rift (text + voice PTT).

## Build Steps

1. Build the Rust shared library for Android:

```bash
rustup target add aarch64-linux-android
```

2. Build `librift_sdk.so` (requires Android NDK toolchain in your PATH):

```bash
cargo build -p rift-sdk --release --target aarch64-linux-android
```

3. Copy the shared library into the app:

```bash
mkdir -p android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/librift_sdk.so android/app/src/main/jniLibs/arm64-v8a/
```

4. Build the APK from Android Studio or CLI:

```bash
cd android
./gradlew assembleDebug
```

## Notes

- The app uses JNI to call into `rift-sdk`.
- Press and hold the PTT button to transmit audio.
- The app assumes a single active channel/session.
