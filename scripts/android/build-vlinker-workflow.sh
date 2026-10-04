#!/bin/sh
# Build the canonical pinned workflow, independently of the original firmware engine.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
core=${OPENSAAB_VLINKER_CONNECTION_CORE:-${OPENSAAB_SIMULATOR_CONNECTION_CORE:-}}
if [ -n "$core" ]; then
    test -f "$core" || { echo "Missing explicit vLinker connection core: $core" >&2; exit 2; }
    exit 0
fi
source_root=${OPENSAAB_VLINKER_SOURCE_ROOT:-"$repo/../OpenSAAB/emulator"}
test -f "$source_root/Cargo.toml" || { echo 'The canonical OpenSAAB/emulator checkout is required for vLinker. Set OPENSAAB_VLINKER_SOURCE_ROOT to its location.' >&2; exit 2; }
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed Android NDK directory}"
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Run this script on macOS or Linux.' >&2; exit 2 ;;
esac
linker="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin/aarch64-linux-android26-clang"
test -x "$linker" || { echo "Missing NDK linker: $linker" >&2; exit 2; }
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$source_root=/opensaab/emulator --remap-path-prefix=$HOME/.cargo=/cargo"
exec cargo build --manifest-path "$source_root/Cargo.toml" --target-dir "$repo/target/vlinker-workflow" \
    --locked --release --no-default-features --target aarch64-linux-android --bin opensaab-connection
