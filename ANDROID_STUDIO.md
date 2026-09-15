# Build OpenSAAB T2

## Requirements

- Rust 1.93 or newer; `rustup target add aarch64-linux-android`.
- Python 3 and a JDK compatible with Android SDK tools. JDK 21 is recommended for Android Studio/Gradle; the custom build was tested with Studio's bundled JBR 25.
- Android SDK platform 36 and build-tools 36.0.0.
- Android NDK r29. Set `ANDROID_NDK_HOME` to its directory.

Set `ANDROID_HOME` to your SDK and `JAVA_HOME` to your JDK. The custom scripts default to Android Studio's standard macOS locations if omitted. They also support Linux with these variables set.

## Rust + APK

```sh
sh scripts/android/build-headless.sh
OPENSAAB_BUNDLE_SUPPORT=0 python3 scripts/android/build-tech2-app.py
```

This produces a debug APK at `target/android-tech2-app/opensaab-tech2.apk`. A firmware-free APK still needs supported files imported before it can run the guest firmware.

For the three-file support profile, place your separately obtained, authorized inputs under a private directory with these relative paths:

- `eprom.bin`
- `extracted/opsys.dwn`
- `dumps/candi/candi.bin`

Then set `OPENSAAB_SUPPORT_ROOT` to that directory. The exact sizes and SHA-256 hashes are in `android/tech2-app/support-files.json`; a mismatch stops packaging. These files are not source-code dependencies licensed by OpenSAAB and are not committed to this repository.

```sh
OPENSAAB_SUPPORT_ROOT=/path/to/private-inputs python3 scripts/android/build-tech2-app.py
```

## Release signing

Use your own signing key for independent builds; it will not update an official signed installation. `scripts/android/init-release-signing.py` creates a local encrypted keystore and a separate password file, without overwriting an existing key. Keep both private and back them up securely.

```sh
OPENSAAB_SUPPORT_ROOT=/path/to/private-inputs OPENSAAB_RELEASE_KEYSTORE=/path/to/key.p12 OPENSAAB_RELEASE_PASSWORD_FILE=/path/to/password.txt python3 scripts/android/build-tech2-app.py --release   --version-name 0.1.0-preview.2 --version-code 100002
```

Release output: `target/android-tech2-release/OpenSAAB-T2-arm64-v8a.apk`.
The custom release builder records the clean source commit in `assets/build.json`, includes license/dependency notices, disables debugging and verifies support hashes. Keep `Cargo.lock` fixed.

## Android Studio

Open the repository root, let Gradle sync, and run `app`. The Gradle build invokes the Rust build and stages native binaries. By default it expects the three support files; use `-PbundleSupport=false` for a firmware-free build. Set `OPENSAAB_SUPPORT_ROOT` in the build environment for external inputs. Gradle's default output is a development build; the official preview uses the explicit custom release command above.

## Checks

```sh
cargo test --locked --no-default-features
python3 scripts/android/test-request-gate.py
```

Device suites use `scripts/android/test-lcd-on-device.py` and a stopped emulator session. Use a disposable Android virtual device for tests that modify app state. No live adapter is required for host unit tests. Synthetic identities in legacy bench fixtures are not vehicle-specific public features.
