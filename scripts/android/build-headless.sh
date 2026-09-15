#!/bin/sh
# Build the emulator CLI for an Android ARM64 device; no desktop GUI dependency.
set -eu

: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed Android NDK directory}"
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Run this script on macOS or Linux.' >&2; exit 2 ;;
esac
toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin"
linker="$toolchain/aarch64-linux-android26-clang"
test -x "$linker" || { echo "Missing NDK linker: $linker" >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repo"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo=/opensaab --remap-path-prefix=$HOME/.cargo=/cargo"
exec cargo build --locked --release --no-default-features \
    --target aarch64-linux-android --bin tech2-emu --bin nano-usb-probe --bin chipsoft-usb-probe
