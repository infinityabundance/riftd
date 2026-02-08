#!/bin/sh
set -eu

ANDROID_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$ANDROID_DIR/.." && pwd)"
JNI_DIR_ARM64="$ANDROID_DIR/app/src/main/jniLibs/arm64-v8a"
JNI_DIR_X86_64="$ANDROID_DIR/app/src/main/jniLibs/x86_64"

mkdir -p "$JNI_DIR_ARM64" "$JNI_DIR_X86_64"

if command -v cargo-ndk >/dev/null 2>&1; then
  echo "Building rift-sdk for Android (arm64-v8a, x86_64) with cargo-ndk..."
  export CMAKE_POLICY_VERSION_MINIMUM=3.5
  export CMAKE_ARGS="${CMAKE_ARGS:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5"
  export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-lc++_shared"
  (cd "$REPO_DIR" && cargo ndk -t arm64-v8a -t x86_64 -o "$ANDROID_DIR/app/src/main/jniLibs" build -p rift-sdk --features android --release)
  exit 0
fi

cat <<'EOF'
cargo-ndk not found.

Install it with:
  cargo install cargo-ndk

Then re-run:
  ./build-rust-android.sh
EOF
exit 1
