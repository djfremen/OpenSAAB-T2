# Third-party licensing inventory

Original OpenSAAB code uses MPL-2.0. This does not replace dependency licenses.
Retain notices when redistributing dependencies or binaries containing them.

Direct Cargo dependencies checked against downloaded package metadata:

| Component | Locked version | Declared license |
|---|---|---|
| m68k | 0.12.1 | MIT |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| minifb (optional desktop GUI) | 0.25.0 | MIT OR Apache-2.0 |
| libc (Unix targets) | 0.2.189 | MIT OR Apache-2.0 |

The Android no-default-features dependency set currently contains 12 external
Cargo packages, including build-time/procedural-macro dependencies. Their original
license texts are preserved in [Android Cargo notices](licenses/ANDROID_CARGO_NOTICES.txt).
This inventory selects the available MIT terms where offered and preserves the
additional Unicode-3.0 license for unicode-ident. It includes extra alternate
license texts for clarity; it does not remove upstream notices.

No missing or incompatible license declaration was found in this Android Cargo
set for the proposed MPL-2.0 licensing. This is a metadata/notice review, not an
audit of the origin of every line of third-party code. Desktop dependencies and
platform/runtime components need their applicable notices in their own releases;
the Android Cargo list is not a complete desktop/SDK inventory.

Original firmware inputs, card images, Tech2Win installer/executable/language
resources and adapter vendor DLLs are separate third-party material, not covered
by OpenSAAB's license. The three-file support manifest records their identity,
not a license grant. See [LICENSING.md](LICENSING.md) for the boundary.

## Android report review lifecycle libraries

Report review uses AndroidX Fragment 1.6.2 and Lifecycle ViewModel 2.6.2 and
transitive AndroidX libraries, under the Apache License 2.0 (The Android Open Source
Project). Their Kotlin standard library/coroutines dependencies are also under
Apache License 2.0 (JetBrains). `com.google.guava:listenablefuture` is under
Apache License 2.0 (Google); JetBrains annotations are under Apache License 2.0.
The pinned Gradle dependency graph is used by both SDK-command-line and Android
Studio builds. No credentials, native adapter driver or remote diagnostic service
is supplied by these libraries.

License: https://www.apache.org/licenses/LICENSE-2.0
AndroidX source: https://android.googlesource.com/platform/frameworks/support/
Kotlin source: https://github.com/JetBrains/kotlin
Coroutines source: https://github.com/Kotlin/kotlinx.coroutines

The ARM64 Bluetooth connection worker additionally uses the dependencies and full
license texts listed in `licenses/ANDROID_BLUETOOTH_CARGO_NOTICES.txt`. Its shared
OpenSAAB workflow and translation source are MPL-2.0 and accompany the release
in the explicitly named Bluetooth matching-source archive.

## Nano transient PASSTHRU initializer

`src/adapters/vcx_nano/init_handshake.rs` is a Rust adaptation of OpenVCX's
published `dll/device_vcx.c` initialization protocol, under LGPL-3.0-only.
Copyright (c) 2026 Erik Fuller and OpenVCX contributors. Source:
https://github.com/erik683/OpenVCX, commit
`d9387049d82232e4d88e236ad2e0c96a48b787ec`.

The complete LGPL-3.0 text is retained in `licenses/OPENVCX_LGPL_3_0.txt` and
its referenced GPL-3.0 text in `licenses/OPENVCX_GPL_3_0.txt`. The port's source
origin, proper two-phase random DH modification and qualification boundary are
in `licenses/OPENVCX_SOURCE_ORIGIN.txt`. Matching source must include these
files and the initializer. Original OpenSAAB transport and application files
retain MPL-2.0. Cargo identifies this combined source as
`MPL-2.0 AND LGPL-3.0-only`; the initializer is not relicensed as MPL.

This component selects an existing device record through the normal published
protocol. It supplies no vendor DLL, firmware image, key, startup capture or
replayed authorization payload.
