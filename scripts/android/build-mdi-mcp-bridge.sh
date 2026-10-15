#!/bin/sh
# Extend the matching preview.70 MDI engine with the shared guest-menu publisher.
set -eu
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to the installed NDK}"
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Build on macOS or Linux.' >&2; exit 2 ;;
esac
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin/aarch64-linux-android26-clang"
test -x "$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo=/opensaab --remap-path-prefix=$HOME/.cargo=/cargo"
exec cargo build --locked --release --no-default-features --target aarch64-linux-android \
    --manifest-path "$repo/core-mdi/mdi-firmware-engine/OpenSAAB-T2/Cargo.toml" --bin tech2-emu
