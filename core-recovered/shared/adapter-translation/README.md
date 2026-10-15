# Shared adapter translation, 0.1.0

Canonical source: `OpenSAAB/shared/adapter-translation`. This independently authored
MPL-2.0 crate supplies common frame validation, representations, capability/session
contracts, bounded classical ISO-TP receive assembly and GM A9/81 report collection.
It opens no adapter, selects no physical bus and performs no vehicle I/O.

Read the [implementation specification](SPEC.md) before extending an adapter,
application workflow or virtual target. It records actual source adoption,
ownership rules, exact pins, verification and work that remains unqualified.

Every adapter implementation retains its USB/Bluetooth/Wi-Fi binding and command
dialect below this boundary. The shared layer distinguishes STN, MIC and other
families as metadata; it does not translate an arbitrary ST command into a VT command
or infer electrical capabilities from a family name. Supply exact, qualified profile
capabilities. Raw GM report mode requires automatic CAN flow control to be disabled;
ordinary ISO-TP selects exactly one host or adapter owner. Original guest firmware
retains its own raw frames and flow control.

`RawFrame` preserves bus, address width, RTR, DLC and payload. `Packet` distinguishes
raw CAN from an assembled ISO-TP payload; assembled messages cannot become guest
frames. `Observation` records session, caller-normalized monotonic time, optional
device timestamp and physical/replay/model/firmware origin. Existing low-level
driver logs retain their original clocks/flags; the layer does not invent a common
wire timestamp or prove that an adapter captured every bus frame.

`host::Session` is one explicitly addressed transaction. Use separate objects for
different ECU/session targets. It ignores different bus/address/session traffic,
enforces idle and absolute deadlines, preserves pending responses, and rejects
malformed sequences, missing completion and stale use of a terminal session.
`SendFlowControl` is an effect for a host-owned transaction; the caller must send
it through its existing command guard and handle I/O failure, cancellation and
settings restoration. This package is a receiver/collector, not a complete generic
ISO-TP sender or adapter settings manager.

Classical CAN, normal addressing and 12-bit ISO-TP lengths are implemented. CAN FD,
ISO-TP extended/mixed addressing, arbitrary signals, physical SW-CAN wake, electrical
arbitration and the full vehicle topology remain outside this package. A 29-bit CAN
ID is distinct from ISO-TP extended addressing. Unknown behavior must stay explicit.

`gm::Report` and `isotp::VinReply` preserve existing OpenSAAB entry points. GM end
records require `81 00 00 00 FF`; record counts and raw failure/status bytes are
preserved. A report without this terminal marker cannot become an empty success.

`SOURCE_SHA256` covers Cargo.toml, build.rs and every runtime Rust source; compilation
rejects changed or omitted source. Consumers use `version = "=0.1.0"` and lockfiles.
The vLinker workflow also pins the translation manifest's hash. The tech2-emu
consumer uses a reviewed snapshot imported with `scripts/import_adapter_translation.py`.
Edit canonical source, review/update its source pin, and deliberately update imports;
never maintain a second protocol implementation in the snapshot or a native binding.

```sh
cargo test --manifest-path shared/adapter-translation/Cargo.toml --locked
cargo clippy --manifest-path shared/adapter-translation/Cargo.toml --all-targets --locked -- -D warnings
```

The contract tests use synthetic inputs and profiles. Their cross-family equality
checks do not qualify Carista, OBDLink, WiCAN, BlueDriver or any actual retail device.
See `EVIDENCE.json` and the owner development report for the provenance and scope of
each design choice. Installed platform artifacts retain their original source pins.
