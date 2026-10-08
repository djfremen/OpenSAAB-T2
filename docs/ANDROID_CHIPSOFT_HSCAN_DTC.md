# Direct Trionic 8 engine DTC read over HS-CAN

The Actions menu offers **Read engine codes — HS-CAN** on the home screen and in Chipsoft sessions. This reads current/history codes from a powered Trionic 8 ECM using Chipsoft Pro, independently of original firmware menu navigation and SW-CAN. A running Chipsoft firmware session is stopped and its USB transport released before the direct reader starts. No other adapter's command path is changed.

A fresh VIN is read first. The report retains that VIN, UTC read time, DTC codes, failure-type bytes and status bytes. Text and JSON reports are saved under app-private `dtc-reports`; the existing Saved DTC reports interface can review/share the text. No automatic report upload occurs. Optional VIN enrichment uses the existing explicit checkbox.

## Wire sequence and ownership

- HS-CAN only, protocol 5 at 500000 bit/s, transmit address 0x7E0.
- Start diagnostic operation: `02 10 02 00 00 00 00 00`, acknowledge on 0x7E8 (`01 50`).
- Request current/history DTCs: `03 A9 81 12 00 00 00 00`.
- Accept pending `7F A9 78` on 0x7E8; decode `81` records on 0x5E8 using shared `t8_dtc::Report`.
- Require the explicit zero-DTC end marker. Silence/partial reports are errors, not zero codes.
- Always attempt stop diagnostic operation: `01 20 00 00 00 00 00 00`, require `01 60` acknowledgement.
- Close channel/device and release Android USB before reporting success.

`src/chipsoft_dtc.rs` owns the Chipsoft session orchestration; `src/t8_dtc.rs` remains the platform/adapter-independent decoder. `ChipsoftCommandGate.engineDtc` permits the three exact requests and HS-CAN controls only, capped at three vehicle transmissions in the DTC phase. The separate VIN phase retains its existing two-request limit. No DTC-clear, security-access, actuator or programming command is included.

Protocol cross-check: downloaded mattiasclaesson/Trionic C# repository, `TrionicCANLib/Trionic8.cs` methods `ReadDTC`, `StartSession10`, `Send0120`; `CAN/J2534CANDevice.cs` specifies 500 kbit/s. This implementation uses the existing OpenSAAB framing/decoder rather than importing the C# library.

## Validation

Host Rust library suite: 113 passed, 7 ignored. Added synthetic transport tests cover pending plus records/end marker, silence, missing end marker, negative response, wrong CAN ID, empty report and missing stop acknowledgement. The full Java host suite passes; added USB-gate tests reject clearing, security, SW-CAN, wrong ECM address, changed mask, truncation and checksum corruption. All Android Java sources compile against API 36.

Physical bench results are recorded separately after installation. These checks do not establish support for other engine types or whole-vehicle module coverage.
