#!/bin/sh
# Build MDI separately while preserving the existing emulator/adapter payloads.
set -eu
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to the installed NDK}"
: "${OPENSAAB_MDI_ENGINE_SOURCE:?Set OPENSAAB_MDI_ENGINE_SOURCE to the matching MDI engine source}"
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo 'Build on macOS or Linux.' >&2; exit 2 ;;
esac
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin/aarch64-linux-android26-clang"
test -x "$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
cargo build --locked --release --no-default-features --target aarch64-linux-android \
    --manifest-path "$OPENSAAB_MDI_ENGINE_SOURCE/Cargo.toml" --bin tech2-emu
cargo build --locked --release --target aarch64-linux-android \
    --manifest-path "$repo/android/adapters/mdi/native/Cargo.toml"
printf '%s\n' "OPENSAAB_MDI_MODULE=$repo/android/adapters/mdi/native/target/aarch64-linux-android/release/libopensaab_mdi_android.so"
printf '%s\n' "OPENSAAB_MDI_BRIDGE=$OPENSAAB_MDI_ENGINE_SOURCE/target/aarch64-linux-android/release/tech2-emu"
