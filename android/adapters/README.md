# Android adapter owners

`chipsoft/` and `vcx_nano/` own their respective USB setup, command gates, activity lifecycle and profiles. `AdapterCatalog` selects typed profiles; common `UsbBridgeCodec` handles only socket bytes. No initialization runs during descriptor inventory or offline emulation.

Classes keep their existing `com.opensaab.usb` package/component names for manifest and session-helper compatibility. The Android build finds source files recursively. See `docs/adapters/README.md` for the full contract and coverage limits.
