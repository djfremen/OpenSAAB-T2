# Third-party licensing inventory

Original OpenSAAB code uses MPL-2.0. This does not replace dependency licenses.
Retain notices when redistributing dependencies or binaries containing them.

Direct Cargo dependencies checked against downloaded package metadata:

| Component | Locked version | Declared license |
|---|---|---|
| m68k | 0.12.1 | MIT |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| minifb (optional desktop GUI) | 0.25.0 | MIT OR Apache-2.0 |
| libc (macOS target) | 0.2.189 | MIT OR Apache-2.0 |

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
