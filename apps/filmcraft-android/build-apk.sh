#!/usr/bin/env bash
# Build the FilmCraft Android APK. Needs: Rust (rustup), Android Studio or the Android SDK + NDK
# (ANDROID_HOME and ANDROID_NDK_HOME set), JDK 17, and a Gradle wrapper (see README).
set -euo pipefail
cd "$(dirname "$0")"
MODE="${1:-debug}"   # debug | release

rustup target add aarch64-linux-android
command -v cargo-ndk >/dev/null || cargo install cargo-ndk --locked

# 1. the Rust library -> android/app/src/main/jniLibs/arm64-v8a/libfilmcraft_android.so
PROFILE=(); [ "$MODE" = release ] && PROFILE=(--release)
cargo ndk -t arm64-v8a --platform 29 -o android/app/src/main/jniLibs build "${PROFILE[@]}" -p filmcraft-android

# 2. the APK
cd android
if [ "$MODE" = release ]; then ./gradlew assembleRelease; else ./gradlew assembleDebug; fi
echo "APK: $(find app/build/outputs/apk -name '*.apk' | head -1)"
