# Chipsoft Pro — Windows Trionic 8 DTC reader

Adapter-specific native J2534 reader. Uses the installed Chipsoft Pro x86 DLL;
no Nano initialization or driver substitution. This is an OpenSAAB host diagnostic
utility, not a successful original Tech 2 firmware-menu test. The current Windows
MSI and launcher have not been changed; menu integration remains pending.

Read-only sequence on raw HS-CAN 500 kbit/s: 7E0 `02 10 02`, then `03 A9 81 12`
(current/history), then `01 20`. Accept real 7E8 session acknowledgments and 5E8
DTC records, require the end marker, stop the session, remove both filters,
disconnect and close. No clearing, security access, flashing or SW-CAN commands.
Chipsoft empty-queue compatibility is gated by exact return code, zero count,
successful GetLastError, and ERR_EMPTY_MESSAGES_QUEUE text.

## Invocation

Run through the existing parent watchdog, which holds the shared adapter lock
and limits the x86 child to 25 seconds. Close other diagnostic programs first.
From an execution-policy-approved PowerShell context:

```powershell
& ([scriptblock]::Create((Get-Content .\emulator\scripts\windows\invoke_vcx_t8_vin.ps1 -Raw))) `
  -ProbePath .\emulator\scripts\windows\chipsoft\read_t8_dtc.ps1 `
  -LogDirectory "$env:LOCALAPPDATA\OpenSAAB\WindowsData\chipsoft-dtc" `
  -TimeoutSeconds 25
```

For a hardware-free compatibility/ABI check, set
`$env:OPENSAAB_CHIPSOFT_DTC_SELFTEST='1'` before invoking the same watchdog;
remove that variable before a live read. Retain the complete parent stdout.
Validate the real raw RX transcript with the emulator's shared Rust decoder:

```sh
cargo run --manifest-path emulator/Cargo.toml --bin vcx-dtc-inspect -- complete-parent-output.log
```

## Bench validation — September 20, 2026

EliteBook Windows 10, Chipsoft Pro detected as USB Serial Device COM7,
VID0483/PID5740, manufacturer J2534 DLL installed. Two live reads returned
P0107, P0122, P0223, P1682, P2122, P2127, each failure type 00. Status bytes
were 6F, FD, FD, 7D, FD, FD respectively. Both reports had a real end marker,
positive session-stop reply, and successful filter/channel/device cleanup.
The final read took about 705 ms within the native helper, excluding PowerShell
startup and compilation. Nothing was cleared.

The first preparation run emitted a PowerShell param warning because the parent
prepends an environment assignment. The self-test selector now uses a dedicated
environment variable; the corrected watchdog self-test and second live run both
exited zero without an error stream. The first raw transcript is retained.

Private evidence: development Mac `~/.local/share/opensaab/windows-chipsoft-dtc-20260920/`;
EliteBook `%USERPROFILE%\OpenSAAB\ChipsoftDtc-20260920\logs`.
This test establishes the native Windows Chipsoft HS-CAN reader, not GUI/menu
integration, an updated MSI, or full vehicle/module coverage.
