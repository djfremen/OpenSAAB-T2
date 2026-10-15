# Private Android carrier probe

This is an owner-operated ARM64 research harness, separate from OpenSAAB's
product UI. [Hardware findings](../docs/ANDROID_PIXEL7_USB_2026-10-05.md) and their
receipt distinguish the two exact tested artifacts.

Build with the installed Android SDK 36/JDK and NDK r29/API 28 linker:

```sh
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android28-clang"
cargo test --locked --manifest-path android/probe/Cargo.toml
cargo clippy --locked --manifest-path android/probe/Cargo.toml -- -D warnings
cargo build --locked --release --target aarch64-linux-android --manifest-path android/probe/Cargo.toml
python3 android/build-probe.py
```

The build requires the explicit known fixture's serial and 56-byte base-key
profile to be provisioned privately as `serial-private.txt` and
`base-key-private.bin` in the application's private files directory. They are
not included in source or APK. No firmware or vendor binary is bundled.

The app has separate explicit inventory, START/VIN and software-reboot actions.
Android USB permission is required again when the device re-enumerates. Evidence
is private per attempt. Each successful invocation releases the MDI and USB;
there is no persistent network service, automatic retry or automatic reconnect.

Protocol source is MPL-2.0. Dependencies are pinned in `probe/Cargo.lock`:
smoltcp 0.12.0/0BSD, JNI 0.21.1/MIT-or-Apache-2.0, libc 0.2.190/MIT-or-Apache-2.0,
and the root OpenMDI management/VIN dependency tree. This development APK is not
a driver or product release. Product packaging/licence notice review, common
UI integration and failure lifecycle qualification remain pending.
