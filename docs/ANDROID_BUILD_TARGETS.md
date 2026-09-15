# Android build targets

The emulator, adapter protocols, reports and UI share one source tree. Architecture-specific packaging is explicit. Do not copy the Rust emulator or Java application into an ARM32 fork.

| Property | Existing ARM64 product | Experimental 32-bit head unit |
| --- | --- | --- |
| Build profile | `arm64` (default) | `headunit-arm32` (explicit only) |
| Rust target | `aarch64-linux-android` | `armv7-linux-androideabi` |
| Android ABI | `arm64-v8a` | `armeabi-v7a` |
| Package | `com.opensaab.tech2` | `com.opensaab.tech2.headunit32` |
| Launcher label | OpenSAAB T2 | OpenSAAB T2 Head Unit 32-bit |
| APK | `OpenSAAB-T2-arm64-v8a.apk` (release) | `OpenSAAB-T2-headunit-armeabi-v7a-dev.apk` |
| APK directory | `target/android-tech2-release` (release) | `target/android-headunit-arm32` |
| Minimum Android | API 26 | API 26; not lowered |
| Signing | Explicit release key or existing development key | Development key only for now |
| Update channel | Existing public ARM64 releases | No public update channel; ARM64 update checks disabled |

The separate package keeps files, preferences, sessions and provider authorities isolated. It cannot update or replace the phone APK. Both apps may appear in a USB permission chooser if installed together; only one should run an adapter session at a time. There are no USB auto-launch intent filters in these manifests.

## Development workspace

Development branch: `feature/headunit-arm32`.
Local worktree: `/Users/mini4/Documents/Projects/OpenSAAB-T2-headunit-arm32`.
Existing public ARM64 checkout: `/Users/mini4/Documents/Projects/OpenSAAB-T2-public`.

The existing published preview APK and release tag remain unchanged. Do not publish ARM32 artifacts under an ARM64 filename or attach experimental builds to that existing release.

## Build the head-unit target

Install the same SDK/JDK/NDK dependencies described in `ANDROID_STUDIO.md`, then:

```sh
rustup target add armv7-linux-androideabi
export ANDROID_NDK_HOME=/path/to/android-ndk-r29
export OPENSAAB_SUPPORT_ROOT=/path/to/private-inputs
sh scripts/android/build-headunit-arm32.sh
```

The same three hash-checked support files are included by default. The Saab card image is not included. To deliberately build without the support files, set `OPENSAAB_BUNDLE_SUPPORT=0`. This does not make firmware unnecessary.

The wrapper builds all three native executables and packages only ARMv7 ELF files. The APK validator verifies actual ELF class and machine identifiers, so renamed ARM64 binaries fail validation. `--release` is deliberately rejected for this experimental target until release signing, versioning and device validation have been established. The build never publishes, installs or contacts a vehicle.

The existing Android Studio `app` configuration still builds ARM64. Use the dedicated command above for ARM32; selecting a head unit in Studio does not change the build architecture.

## Implementation map

- `scripts/android/build_profiles.py`: package/ABI/Rust-target identities and ELF validation.
- `scripts/android/build-headless.sh`: explicit NDK linker and Rust target.
- `scripts/android/build-headunit-arm32.sh`: dedicated head-unit entry point.
- `scripts/android/build-tech2-app.py`: separate output, package, authorities and development label.
- `android/shared/com/opensaab/usb/AppBuildProfile.java`: installed build identity for UI, requirements, logs and update handling.
- `scripts/android/check-apk-firmware.py`: target-specific native payload and support-file checks.

## Validation status and next steps

September 15, 2026: both ARMv7 and ARM64 native builds and development APK packaging passed. Verified distinct package IDs, provider authorities, native ABI directories and three executables per APK. Native ELF mismatch/mixed-ABI rejection tests, Java compatibility/protocol/report tests and standalone checker compilation passed. These are build/host checks; no head-unit installation or live vehicle test was performed.

First ARM32 development APK: 3,278,446 bytes; SHA-256 `8664fe1649c3a116f18d789f1d60fc8ef0cad0f8a0fe778b9ca6b0bf2b47853f`. Locally generated `SHA256SUMS` accompanies each build directory. This artifact is debug-signed and has not been published.

This target is experimental, not a promise of head-unit compatibility. Compilation alone does not prove runtime correctness, USB transport or vehicle operations.

Physical candidates: K2401 (reported API 29, armeabi-v7a, 3.9 GiB RAM, 1280x720); Universal_U01/AC8227L (1 GB RAM, 1024x600; actual API still unverified).

Before release: verify native startup and original firmware menus on real ARM32 Android; audit pointer widths and FFI; test landscape layout and accessible EXIT; measure memory and responsiveness; test Chipsoft identification, VIN/ECU information, DTC and interruption recovery. Adapter screens still retain the existing portrait configuration in this first build separation. Nano, programming and security access need their own validation. Do not equate building the test APK with finishing those tasks.
