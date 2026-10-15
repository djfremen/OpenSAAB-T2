#!/bin/sh
set -eu
: "${ANDROID_NDK_HOME:?Android NDK is required}"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android26-clang"
exec cargo build --manifest-path core-recovered/emulator/Cargo.toml --locked --release --no-default-features --target aarch64-linux-android --bin tech2-emu
