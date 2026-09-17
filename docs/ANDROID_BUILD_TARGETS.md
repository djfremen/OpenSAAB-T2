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
| Signing | Explicit release key or existing development key | Same official release key for signed previews; debug key for development |
| Update channel | Existing public ARM64 releases | Separate experimental headunit tags; ARM64 update checks disabled |

The separate package keeps files, preferences, sessions and provider authorities isolated. It cannot update or replace the phone APK. Both apps may appear in a USB permission chooser if installed together; only one should run an adapter session at a time. There are no USB auto-launch intent filters in these manifests.

## Development workspace

Development branch: `feature/headunit-arm32`.
Local worktree: `$PROJECTS/OpenSAAB-T2-headunit-arm32`.
Existing public ARM64 checkout: `$PROJECTS/OpenSAAB-T2-public`.

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

The wrapper builds all three native executables and packages only ARMv7 ELF files. The APK validator verifies actual ELF class and machine identifiers, so renamed ARM64 binaries fail validation. Signed experimental builds require `--release --version-name 0.1.0-headunit.1 --version-code 100001` and the existing release-signing environment variables. They use `target/android-headunit-arm32-release/OpenSAAB-T2-headunit-armeabi-v7a.apk` and separate `headunit-v…` release tags. They are still physically unvalidated. The build never publishes, installs or contacts a vehicle.

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

Historical first ARM32 development APK: 3,278,446 bytes; SHA-256 `8664fe1649c3a116f18d789f1d60fc8ef0cad0f8a0fe778b9ca6b0bf2b47853f`. Locally generated `SHA256SUMS` accompanies each build directory. This artifact is debug-signed and has not been published.

This target is experimental, not a promise of head-unit compatibility. Compilation alone does not prove runtime correctness, USB transport or vehicle operations.

Physical candidates: K2401 (reported API 29, armeabi-v7a, 3.9 GiB RAM, 1280x720); Universal_U01/AC8227L (1 GB RAM, 1024x600; actual API still unverified).

Before release: verify native startup and original firmware menus on real ARM32 Android; audit pointer widths and FFI; test landscape layout and accessible EXIT; measure memory and responsiveness; test Chipsoft identification, VIN/ECU information, DTC and interruption recovery. Adapter screens still retain the existing portrait configuration in this first build separation. Nano, programming and security access need their own validation. Do not equate building the test APK with finishing those tasks.

## First signed experimental distribution

User authorized an alternate website download on September 15, 2026, without connecting the physical head unit. Publish as prerelease `headunit-v0.1.0-headunit.1`, never as the latest default release. Version code 100001 belongs to the separate headunit32 package. The checker 0.2.0 selects ARM64 first when supported, otherwise ARMv7; below API 26 or on unsupported ABIs it offers no installer. Links open release notes in the browser, never silently install. ARM32 physical-device and adapter validation remain pending. Existing portrait adapter layouts are unchanged.


## Landscape preview update — September 15, 2026

User photos confirm that preview 1 installs and launches on the Universal_U01 / 8227L head unit, with original firmware visible in offline mode. The app reports 1024×600 at 160 dpi, 0.9 GiB RAM, ARMv7 application support and Android API 26 or newer. The exact API number remains unverified. This is user-reported startup evidence, not proof of USB or diagnostic operation on this device. The K2401 is a separate device.

Preview 2 (`0.1.0-headunit.2`, code 100002) fixes the tiny firmware display caused by stacked phone controls consuming the landscape height. `HeadunitLayout.java` puts the firmware display and persistent EXIT/soft keys on the left and scrollable session tools on the right. It applies only to the headunit32 package on landscape windows at least 720 dp wide. Main, Chipsoft and Nano firmware screens share the shell. Portrait phone layout and the existing ARM64 release are unchanged. The ARM32 manifest no longer forces portrait. Images retain their original aspect ratio; no diagnostic protocol or firmware changes.

UI instrumentation uses a disposable Java-only test package on the Mac's ARM64 Android emulator, because that emulator cannot execute ARMv7 native binaries. Tests cover activity layouts, minimum display dimensions, visible EXIT, full keypad, console, scrolling and gesture callbacks; this is not ARM32 engine validation. Actual revised layout, low-memory performance and USB behavior still need physical testing. No vehicle commands were sent for this work.

## Current release distinction

Earlier validation notes above describe those particular builds and dates. User reports later confirmed ARM32 startup on the 8227L (API 27) and Chipsoft/VIN identification; sustained firmware diagnostics and low-memory performance remain unresolved. ARM64 preview.3 brings shared reporting and compatibility UI into the phone channel. ARM32 headunit.3 stays independently published. See ANDROID_FEATURE_MATRIX.md.
