# Shared interface reference

The standing product standard and required visual/workflow evidence are in
[`interface-contract.json`](interface-contract.json) and
[`CONFORMANCE.md`](CONFORMANCE.md). All five targets must pass that same contract
before they can be called a uniform release. See the repository `AGENTS.md`.

The latest Android `SessionWorkspace`, `SessionStyle`, `Tech2Controls` and `MainActivity` define the reference interaction. Desktop ports must preserve the firmware display, key encoder semantics and distinction between offline operation, a detected adapter candidate, and verified live ECU replies.

`firmware-catalog.json` is generated metadata, not OEM firmware. Update it with `tools/ui/export_android_catalog.py`, reviewing the Android source checksum, then use `--check` in validation. All desktop consumers should use this catalog instead of maintaining independent download lists.

Windows 0.2.x and native macOS ARM64 0.2.0 implement this shell reference. The Mac port exposes Chipsoft identity probing only; live VIN/DTC and advanced actions remain incomplete. Linux still uses its earlier frontend. Shell implementation does not prove full Android feature parity. The release matrix remains the authority for qualified features.

Reference controls: S1=0x0a, S2=0x06, S3=0x07, S4=0x08; EXIT=0x01 (navigate back, never stop the process); ENTER=0x10; arrows=0x09/0x0c/0x0b/0x0d. Firmware framebuffer is 320×240 and must retain its 4:3 aspect ratio. App menu owns Stop; the real firmware owns diagnostic menu navigation.
