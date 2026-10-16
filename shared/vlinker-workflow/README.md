# Shared vLinker workflow, version 0.1.3

This is the canonical dependency for Android and iOS VIN-first startup,
Trionic 8 HS-CAN identification/current-history DTC probes, connection ownership,
and durable ECM reports. Java and Swift supply native transport, lifecycle,
storage roots and UI effects; they do not select vehicle commands or decode CAN.
The wider original-firmware emulator is not consolidated by this package.

Both consumers use `version = "=0.1.3"` and a local path to this directory.
Keep `OpenSAAB`, `OpenSAAB-iOS`, and `OpenSAAB-T2-beta-diagnostics` as sibling
checkouts. The Android connection executable is built from canonical
`OpenSAAB/emulator`; iOS links the same package through its emulator crate.
Consumers build with their checked-in lockfiles (`--locked`).

`SOURCE_SHA256` pins Cargo.toml, build.rs and every Rust source. The build
rejects a source change until the package version/pin is deliberately reviewed.
The report manifest records version and SHA-256 of this pin file. Current pin:
`d34004c8c619947b9edb61ea710c5c59791ccda87d425bb060bdce2a8f73d65e`.

Verify locally:

```sh
cargo test --manifest-path shared/vlinker-workflow/Cargo.toml --locked
```

The qualification receipt `releases/consistent-baseline/ANDROID_IOS_VLINKER_STABILITY_2026-10-03.json`
records exact consumer artifacts, tests, source archive hashes and private
checkpoint location. No firmware or vehicle identity is distributed with this
package. Physical iPhone BLE and other architectures require their own installed
qualification; simulator results do not establish those passes.

For transport effects, claim ownership before opening the adapter, apply every
`send` effect including timeout/cancellation cleanup, confirm native release,
then destroy the session. Reconnect creates a fresh session and identity.
Saved reports are observations, never an active vehicle identity. Complete
code reports require the ECM end marker, positive diagnostic stop and confirmed
transport close. Incomplete/cancelled/disconnected probes remain evidence only.

October 3 translation migration: source 0.1.1 uses pinned `opensaab-adapter-translation` 0.1.0 for VIN/identifier assembly and GM reports. `TRANSLATION_SOURCE_SHA256` binds the transitive source pin. The existing installed preview.37/iOS23 qualification belongs to source 0.1.0 and its original receipt. New source tests and core compilation are offline evidence; at that checkpoint, source 0.1.1 had not been installed or tested against physical adapters.

October 3 EVO extension: source 0.1.2 separates discovery hints from product
identity and the qualified command profile. `VTI` selects vLinker MC+ or
Carista EVO; `VTVERS` validates the model-specific MIC family. The owned EVO
reports Carista EVO v2.2.84 / MIC3413 V2.0.84. Every VIN attempt checks protocol
6 and 500000 readback, configures adapter-owned flow control, reads once, then
requires VTPC and confirmed native close. Android records the selected device
and actual model/profile separately. At this VIN checkpoint, EVO ECM probes
were rejected before CAN setup; SW-CAN and original-firmware wireless transport
remained unqualified.
The Pixel 7 exact-artifact receipt is
`releases/consistent-baseline/CARISTA_EVO_PIXEL7_2026-10-03.json`. Other
platforms consume the updated shared source on their next build; their installed
artifacts and hardware qualification retain their original scope.

October 3 EVO ECM extension: source 0.1.3 enables the existing shared Trionic 8
identifier and current/history code reader for EVO firmware MIC3413 V2.0.84.
Other EVO probe firmware fails before CAN setup. Results retain operation-specific
model/profile IDs, raw identifier bytes and GM report status bytes. Each probe
reads a fresh VIN, verifies protocol/baud, sends each vehicle request once and
requires protocol/native close. DTC reads additionally require the explicit GM
end marker and a confirmed diagnostic-session stop before saving. Pixel 7 local
preview.43 completed both physical probes and reopened the saved code report.
Receipt: `releases/consistent-baseline/CARISTA_EVO_PIXEL7_ECM_2026-10-03.json`.
These bounded reads do not qualify SW-CAN, other ECM families, clear/programming
services, original-firmware wireless operation or another installed platform.
