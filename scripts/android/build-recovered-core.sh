#!/bin/sh
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
if [ -n "${OPENSAAB_RECOVERED_CORE:-}" ]; then
    test -f "$OPENSAAB_RECOVERED_CORE" || { echo "Missing explicit recovered core: $OPENSAAB_RECOVERED_CORE" >&2; exit 2; }
    exit 0
fi
: "${ANDROID_NDK_HOME:?Android NDK is required}"
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Run this script on macOS or Linux.' >&2; exit 2 ;;
esac
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin/aarch64-linux-android26-clang"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo=/opensaab --remap-path-prefix=$HOME/.cargo=/cargo"
exec cargo build --manifest-path "$repo/core-recovered/emulator/Cargo.toml" --locked --release --no-default-features --target aarch64-linux-android --bin tech2-emu

