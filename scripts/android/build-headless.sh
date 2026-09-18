#!/bin/sh
# ARM64 remains the default. ARM32 includes the verified fast-start path.
set -eu

profile=${1:-arm64}
case "$profile" in
    arm64) target=aarch64-linux-android; clang=aarch64-linux-android26-clang ;;
    headunit-arm32) target=armv7-linux-androideabi; clang=armv7a-linux-androideabi26-clang ;;
    *) echo 'Usage: build-headless.sh [arm64|headunit-arm32]' >&2; exit 2 ;;
esac

: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed Android NDK directory}"
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Run this script on macOS or Linux.' >&2; exit 2 ;;
esac
toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin"
linker="$toolchain/$clang"
test -x "$linker" || { echo "Missing NDK linker: $linker" >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repo"
case "$profile" in
    arm64) export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker" ;;
    headunit-arm32) export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$linker" ;;
esac
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo=/opensaab --remap-path-prefix=$HOME/.cargo=/cargo"
set --
if [ "$profile" = headunit-arm32 ]; then
    export RUSTFLAGS="$RUSTFLAGS -C target-cpu=cortex-a7"
    export CARGO_PROFILE_RELEASE_LTO=fat
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
    set -- --features load-test
fi
exec cargo build --locked --release --no-default-features \
    "$@" --target "$target" --bin tech2-emu --bin nano-usb-probe --bin chipsoft-usb-probe
