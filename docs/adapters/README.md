# Adapter implementation boundaries

Each adapter owns its USB initialization, wire framing/opcodes, channel setup, receive parsing, error interpretation and cleanup. Shared code handles selection, Android consent, firmware menus, CAN events, session policy and transport bytes. A shared USB chip ID selects a **candidate**, not a proven adapter or working vehicle connection.

## Repository map

| Bucket | Rust | Android owner | Current coverage |
| --- | --- | --- | --- |
| Chipsoft Pro | `src/adapters/chipsoft/` | `android/shared/com/opensaab/usb/adapters/chipsoft/` | Direct CDC USB; captured identity and vehicle-read workflows; exact vehicle/bus limitations still apply |
| VCX Nano | `src/adapters/vcx_nano/` | `android/shared/com/opensaab/usb/adapters/vcx_nano/` | CH343 USB initialization and Nano protocol; earlier successful tests, later channel-initialization regression remains unresolved |
| Windows J2534 | `src/adapters/j2534/` | None | Shared Windows bridge dispatches the selected vendor's helper/driver; this is not an Android USB driver |
| Common | `src/adapters/common/`, `src/can_adapter.rs` | `AdapterCatalog`, `AdapterProfile`, `UsbBridgeCodec` and other shared UI/session classes | Bounded byte transport, original-firmware request policy and CAN events; no vendor command fallback |
| Bosch / ETAS / possible MDI | No Android backend | Inventory entries in `AdapterCatalog` | Detection only. No command implementation selected |

The Rust `lib.rs` re-exports preserve existing CLI/API module names (`chipsoft_backend`, `nano_usb`, `vcx`, etc.). The implementation files now live in their corresponding buckets. The Android bucket files retain package `com.opensaab.usb` deliberately: existing manifest component names, instrumentation and package-private session helpers remain compatible. The build discovers these directories recursively. These are clear ownership boundaries, not separate APKs or security sandboxes.

### Chipsoft Pro

- `protocol.rs`: Chipsoft framing, opcodes, response status decoding.
- `channel.rs`: Chipsoft channel setup, transmit/receive contract and shutdown.
- `backend.rs`: original CANdi requests through Chipsoft and completion/error handling.
- `probe.rs`: bounded identity probe; `serial.rs`: macOS transport.
- `ChipsoftProfile.java`: candidate USB ID `0483:5740`, backend identity and packaged probe executable.
- `ChipsoftUsbActivity.java`: Android USB ownership, CDC interface validation, permission lifecycle and cleanup.
- `ChipsoftCommandGate.java`: validates Chipsoft command bytes before USB writes.

Chipsoft CDC deliberately skips line-coding and DTR/RTS changes. It must never inherit Nano's CH343 initialization sequence. A VID/PID match alone is ambiguous because unrelated STM devices can share it. The original protocol handshake checks actual identity.

### VCX Nano

- `protocol.rs`: Nano framing/escaping, reply deadlines and identity parsing.
- `channel.rs`: Nano channel open/configure/close sequences and raw CAN decoding.
- `native.rs`: Nano-specific encoding, reply handling and transmit bookkeeping.
- `backend.rs`: original CANdi requests through the Nano backend.
- `NanoProfile.java`: candidate USB ID `1A86:55D3`, backend identity and packaged probe executable.
- `NanoProbeActivity.java`: Android CH343 setup, USB ownership and cleanup.
- `NativeCommandGate.java`: existing Nano wire-command gate (legacy class name retained).

Nano owns the captured vendor control requests (`A1`, then `A4` sequence) and its `BB`-delimited wire protocol. None of these commands belongs in generic adapter selection. A CH343 match is a candidate; the Nano identity response and channel results remain necessary.

## Initialization-menu contract

1. Enumerate USB descriptors without opening a device or transmitting.
2. `AdapterCatalog.identify()` returns an optional typed `AdapterProfile` (`CHIPSOFT_PRO`, `VCX_NANO`, or no supported backend). Descriptor-only devices never acquire an executable profile.
3. Display **Chipsoft Pro candidate · Connect and start** or **VCX Nano candidate · Connect and start**. Multiple candidates require selection. No candidate preserves the explicit offline path.
4. After the user's Connect action, revalidate the selected USB path/device ID/VID/PID. Route exclusively to that profile's activity and packaged probe. Show that its driver is opening and identity is being checked.
5. Android USB permission and adapter-specific interface validation occur before initialization. Apply only that adapter's USB sequence. Verify the protocol identity, then attempt supported channel/vehicle operations.
6. Report driver/identity/channel/vehicle failures separately. A recognized adapter does not establish SW-CAN wiring, vehicle compatibility, or a successful ECU reply. Never try another vendor's commands as an automatic fallback.

This change centralizes the routing metadata and launcher dispatch. It does not claim a new working Nano vehicle session. More detailed persistent progress labels (for example “VCX Nano confirmed · configuring channels”) should be driven by actual successful protocol results, not USB descriptors.

## Shared helpers extracted from Nano

`common/usb.rs` owns `UsbTransport` and `SocketUsb`: a bounded socket-to-USB bridge with no vendor opcode interpretation. `common/policy.rs` owns the shared firmware request `Profile` and `CommandGate`. `can_adapter::electrical_route` gives the shared capture-backed electrical mapping; legacy names remain aliases. Android's `UsbBridgeCodec` owns bounded ASCII and hex framing. Chipsoft no longer imports Nano's activity, byte transport or request-policy implementation.

The relocation preserves existing wire sequences, request permissions, deferred CANdi startup and cleanup behavior. Offline emulation does not select or initialize either adapter.

## Adding an adapter

Create a named bucket with its profile, identity handshake, transport setup, command codec, channel behavior, receive decoder and cleanup. Register only evidence-backed descriptor hints. Keep unsupported/detection-only profiles without executable backends. Add tests for malformed frames, wrong model/firmware, rejected channels, timeouts, disconnects and cancelled permission; record hardware evidence independently. Do not infer full support from an installed driver or USB identity.

## Validation for this change

- 110 Rust library tests passed; 7 hardware/capture-dependent tests remained ignored.
- All command-gate Java tests passed, including adapter catalog/routing and bounded shared codec checks.
- `test-adapter-boundaries.py` rejects direct cross-dependencies between the Chipsoft and Nano implementations.
- Rust binaries checked; ARM64 and ARM32 Android library targets checked.
- Both Android development APKs compiled with recursive source discovery.
- No new live adapter test was performed: the user's adapter was busy. This repository change is not in the published preview.22/headunit.17 APKs and was not substituted for the public build recorded on Pixel.
