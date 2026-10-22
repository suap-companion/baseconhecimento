#!/bin/sh
# Compila a biblioteca Rust do aplicativo para as 3 ABIs do Android e gera as ligações Kotlin.
#
# Usa Docker (a imagem de docker/android-build.Dockerfile) e o NDK Linux, instalado com
#   android --sdk=~/.android-sdk-linux sdk --platform=linux_x86_64 install ndk/29.0.14206865
# Saída: android/app/src/main/jniLibs/<abi>/libbaseconhecimento_mobile.so e
#        android/app/src/main/java/uniffi/baseconhecimento_mobile/baseconhecimento_mobile.kt
set -eu
cd "$(dirname "$0")/.."
NDK="${NDK_LINUX:-$HOME/.android-sdk-linux/ndk/29.0.14206865}"

docker build -q -t baseconhecimento-android-build -f docker/android-build.Dockerfile docker >/dev/null
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$PWD":/w -w /w \
  -v "$NDK":/ndk:ro \
  -v cargo-reg:/usr/local/cargo/registry -v android-target:/w/target \
  baseconhecimento-android-build sh -c '
    set -e
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -P 26 \
      -o android/app/src/main/jniLibs build --profile mobile -p baseconhecimento-mobile
    # As ligações saem da biblioteca do host: a do Android, com `strip`, perde os metadados do UniFFI.
    cargo build -q -p baseconhecimento-mobile
    cargo run -q -p baseconhecimento-mobile --features bindgen --bin uniffi-bindgen -- generate \
      --library target/debug/libbaseconhecimento_mobile.so \
      --language kotlin --no-format --out-dir android/app/src/main/java
  '
ls -l android/app/src/main/jniLibs/*/libbaseconhecimento_mobile.so
