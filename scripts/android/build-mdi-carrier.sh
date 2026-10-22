#!/bin/sh
# Build the Android JNI/USB carrier independently from its firmware engine.
set -eu
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to the installed Android NDK}"
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) exit 2 ;;
esac
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin/aarch64-linux-android26-clang"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo=/opensaab --remap-path-prefix=$HOME/.cargo=/cargo"
exec cargo build --locked --release --no-default-features --target aarch64-linux-android \
    --manifest-path "$repo/android/adapters/mdi/native/Cargo.toml" --lib
