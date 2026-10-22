# Retained-input ARM64 preview builds

`scripts/android/build-preview.py` is the tracked build and retention entry point
for the candidate declared in `android/tech2-app/preview-build.json`. It requires
a clean committed checkout and creates a new output directory outside the source
tree. It builds the root/MX, recovered and MDI firmware engines plus the MDI
JNI/USB carrier from that exact commit, with matching Cargo/lockfile/runtime
identities. These four crate builds provide five native entries. Root and MX role names
deliberately carry identical common-engine bytes.

Three unchanged workers have explicit byte/source pins: Nano and Chipsoft probes
retain their preview.1 source identity; the connection
worker retains its preview.26 source identity. Their original provenance is
separate from the new application's version. The source-pinned preview.30 APK
can supply these inputs, or supply their exact bytes from a retained native
companion under `lib/arm64-v8a/`. The builder rejects any substituted worker.
Historical retained-native files and APKs are preserved. The build also archives
the exact original source commits for retained workers, separately named and
hashed; the checkout must contain those historical commits (fetch them before
building from a shallow clone).

Use the SDK/JDK/NDK requirements in `ANDROID_STUDIO.md`, plus a JDK21 Gradle
runtime (`OPENSAAB_GRADLE_JAVA_HOME`). Set `OPENSAAB_RELEASE_KEYSTORE` and
`OPENSAAB_RELEASE_PASSWORD_FILE` for your signer. The output is a locally signed,
non-debuggable private developer preview; this command does not install or publish.

```sh
python3 scripts/android/build-preview.py \
  --retained-apk /absolute/path/to/the-pinned-preview.30.apk \
  --output /absolute/path/outside/the-checkout/preview32
```

The default package is firmware-free. `--support-root /authorized/private/inputs`
opts into the three separately obtained, size/hash-pinned support files. No Saab
card is bundled. Use `--retained-native-dir /path/to/native-components` instead
of `--retained-apk` to reuse the native companion's pinned members.

The builder resolves fresh Gradle AAR/JAR inputs, verifies the exact dependency
graph and bytes against `report-dependencies.lock.json`, and hashes registry
Rust dependency sources as well as Cargo locks. Original AAR/JAR inputs are
copied into the packaging receipt directory, and registry Rust source inputs
have a separate retained archive. Toolchain, dependency, native
component, command, signature and packaged-identity receipts accompany the APK,
matching source archive and native companion. `candidate-manifest.json` binds
their SHA-256 values to the clean source revision. No previous compiled report
staging or previous resource table is required.

The standard `build-tech2-app.py` also accepts `--native-input-manifest` and
`--output-dir`. Every native input must be present, ABI-correct and match its
declared SHA-256; rebuilt components must name the current source and full version.
An older retained recovered core is permitted only with its explicit original
source/version and source-package hash. This provenance is checked again against
the final APK. Gradle can consume that same manifest through
`OPENSAAB_NATIVE_INPUT_MANIFEST`, preserving the exact staged bytes.

```sh
python3 scripts/android/test-build-provenance.py
python3 scripts/android/test-native-payload-baseline.py
```

A completed build verifies the source/artifact link and exact packaged bytes.
An independent byte-for-byte second rebuild, installed target identity, adapter
hardware behavior and vehicle workflows remain separate evidence. The manifest
records these as unperformed until their own checks exist. The private signing
key is required to reproduce the same signing identity; independent signers can
build their own package without updating an existing official installation.
