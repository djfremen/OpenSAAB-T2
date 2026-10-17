# OpenSAAB Windows 0.2.18-preview.5 matching source

Scoped Windows x86_64 preview: recognize the active Saab 148.000 welcome before
deferred CANdi startup, report the installed shell version/revision, and use
the developer-preview security workflow without an operator password prompt.
Internal service authentication, consent, cancellation and import validation remain.

This is an incomplete development preview. Physical adapter, vehicle/security,
native x64, MSI and full interface-conformance checks remain pending. No claim
is made to fix CIM authorization, return code -2014, FIFO loss or Chipsoft cold
startup. Fusion testing uses the x64 program under Windows 11 ARM emulation.

Build from this clean committed source on macOS with Rust's
x86_64-pc-windows-gnu target, MinGW and .NET SDK 9:

```sh
python3 tools/release/build_windows_preview.py --baseline /path/to/OpenSAAB-0.2.17-x64-dev-preview.zip --output /outside/source/candidate
```

The baseline is hash-pinned to the published Windows 0.2.17 package. The builder
rebuilds the engine, shared security/report/connection workers, Chipsoft workers
and WinForms shell. It retains the baseline's two vendor comparison bridges,
approved support images and notices; no diagnostic card is bundled. Historical
bridge source is retained under baseline-source. Old MSI build.ps1 instructions
are historical and do not produce this portable preview. Read the deployment
rules before any application publication.

Offline relaunch now skips security restoration only when the selected prior offline session has a fully cleared SSA. Existing collected/imported data keeps its normal validation.
