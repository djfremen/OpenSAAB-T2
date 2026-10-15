# Adapter Rust ownership

- `chipsoft/`: Chipsoft-only framing, `init.rs`, CAN codec, session backend, identity and CDC transport.
- `vcx_nano/`: Nano-only framing, `init.rs`, CAN codec, status/ticket handling and session backend.
- `j2534/`: Windows provider bridge and J2534 wire encoding.
- `mongoose/`: GM II USB framing, captured startup reference, offline USBPcap/VIN decoder and bounded Android connection-beta worker in `live.rs`; physical startup, VIN/DTC and original-firmware transport remain pending. See [startup mapping](../../docs/adapters/MONGOOSE_STARTUP_MAPPING_2026-10-05.md) and [Android beta](../../docs/adapters/MONGOOSE_ANDROID_BETA_2026-10-05.md).
- `mdi/`: bounded offline native TCP trace decoding plus classic Bosch MDI Windows profile and x86 ABI; assembled host VIN plus adapter-owned raw lifecycle, Bosch wake metadata, guarded writes, reverse cleanup and bounded guest server; opt-in `polling.rs` owns exact captured body start/stop vectors and bounded uncertain-start cleanup. Native I-bus/fresh guest VIN passed in a private Windows checkpoint; full DTC, cold/product UI and MDI2 remain unqualified.
- `common/`: bounded bytes and original-firmware request policy; no vendor commands or flags.

The neutral CAN boundary is `src/can_adapter.rs`. MC+ and EVO each own a model
profile in canonical `OpenSAAB/shared/vlinker-workflow/src/adapters/`; their MIC
protocol engine is shared. Canonical emulator `vlinker.rs` is a facade.

See [architecture, initialization and qualification](../../docs/adapters/README.md).
Existing public module paths remain compatibility re-exports in `src/lib.rs`.
