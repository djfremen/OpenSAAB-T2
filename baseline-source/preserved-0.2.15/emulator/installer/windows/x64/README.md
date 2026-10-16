# Windows preview 0.2.15 — public preview with shared security processing

This version removes a historical private vehicle identifier from the experimental
OEM helper and disables its obsolete bench-only authority loader. Direct USB/CDC,
shared security processing and original-firmware operation are unchanged from
0.2.14. The OEM route remains comparison-only.


The Windows shell now exposes the existing shared Chipsoft modes. Direct USB/CDC
starts in Full native control by default; an explicitly saved Read-only diagnostics
preference is retained. Collect security seeds uses the shared restricted seed
profile. The OEM comparison route remains read-only. Mode changes require a
stopped session and are recorded in each run's `transport.json`.

Original-firmware Get Security Access can collect seed data on the direct route.
The status reader prefers the shared `working-ssa.bin` checkpoint so it sees the
current collection rather than an older card image. A truncated checkpoint is
unavailable, never a reason to fall back to older authorization.

Get Security Access now uses the shared Android-derived Rust workflow already
consumed by the Mac shell. Existing PRE-AUTH data can be processed without another
VIN read or seed collection. The Windows binding presents the service-password
and data-sharing prompts, sends credentials only to OpenSAAB, and offers one
explicitly permitted outage-only Bojer fallback. Optional remembered passwords
use Windows DPAPI for the current user. They are never sent in worker arguments
or included in reports.

The emulator stops before import. Shared validation binds the selected collection,
vehicle observation, firmware, response, cancellation and working card. Only the
714-byte SSA region changes in a new card; the original and collection evidence
remain available. Windows process-liveness and exclusive import-lock bindings
prevent importing into an active session. POST-AUTH and its timestamp describe
the imported card. When the original adapter port/model reply is available,
firmware resumes automatically without a new VIN read while ignition may be off.
GET_INFO is a model/firmware check, not a unique hardware identity.

Fresh collection uses the shared menu navigator and a new working card; the user
follows physical ignition prompts. Manually reaching the original TIS transfer
screen exposes the same processing action. Reopening firmware restores matching
security data and its receipt. Exact installed-artifact evidence and remaining
vehicle checks are recorded separately; this is not a complete platform-parity
or public-release claim.

App menu → Report issue now prepares a private JSON through the shared Rust
collector, presents it for review and offers Send to OpenSAAB, Save JSON, Copy JSON
or Keep private. HTTPS upload is bounded, does not follow redirects, and only
reports success after the shared receipt validator confirms server storage.
The exact reviewed JSON is retained locally; failures allow an explicit retry.
Raw CAN traffic, VINs, security blocks and screenshots are excluded by the shared
collector. Free-text descriptions are user supplied.

## Previous baseline


**0.2.11 private bench build:** Shared adapter-stop reasons now reach the desktop
shell, stopped firmware frames are cleared, and failure explanations wrap for
readability. Runtime checks use Windows delete sharing when reading atomically
replaced output files. See `releases/consistent-baseline/WINDOWS_0211_RECEIPT_2026-09-24.json`
for the exact package, test results and remaining limitations.

**Transport policy (since 0.2.8):** OpenSAAB direct USB/CDC is now the default and the only
supported route, matching Android and macOS. The vendor J2534 route is labelled
**“Experimental — OEM comparison”**: it exists for internal A/B tests against the
vendor stack and must not appear in public supported-adapter claims. VCX Nano has
no supported Windows route. The installed 0.2.8 bench qualification is recorded in
`releases/consistent-baseline/WINDOWS_028_RECEIPT_2026-09-24.json`.

**0.2.7 baseline (tested, private EliteBook MSI `5bd1a5cb…41f5`):** exact inputs are
commit `d9f03f6` on `windows-ab-0.2.7`. The notes below describe that A/B build.

The 0.2.7 source added **App menu → Adapter & advanced tools → Transport**.
In 0.2.7 OEM J2534 was the default; the x64 app/emulator uses a separately compiled **x86**
helper to load the installed 32-bit manufacturer DLL. The alternative is the
existing shared Chipsoft USB/CDC implementation. VCX Nano was OEM-only.
Both startup VIN identification and firmware use the selected route. A missing
driver/helper or invalid COM port fails explicitly; there is no silent fallback.
OEM DLL and helper PE architecture are checked before live firmware starts.

Connect reads a fresh VIN and starts firmware without the previous automatic
OEM-only DTC probe, so that extra diagnostic session does not confound the
comparison. The standalone OEM engine-DTC reader remains available through
Actions. It cannot silently change a direct-transport session to OEM.
Both transports use the shared interactive key mailbox, live framebuffer and
context-sensitive ENTER handling. Local OEM runs are marked live, so offline
checkpoint recovery cannot rewind a hardware session.
Transport selection cannot change while an operation or firmware session runs.

Each firmware run saves `transport.json`: route, adapter, firmware/emulator/bridge
hashes, DLL file version/hash, time, and policy scope, with no VIN or security data.
These are **read-only comparison previews**. The two backend command gates are
not asserted identical; a local gate rejection must be distinguished from an ECU
response. Security processing/full control and general Android parity remain open.
Startup helpers have different setup/teardown paths: compare firmware-menu traffic separately from those startup probes.

The builder now compiles both bridge helpers explicitly with `/platform:x86`.
The MSI remains x64 and does not support 32-bit Windows. Compile checks and
portable routing tests passed on the Mac. Native Windows core tests, shell tests
and English/German firmware runtime checks passed. The private MSI was installed
on the EliteBook, all 16 packaged file hashes matched, and all 85 pre-existing
user-data files were preserved. Installed welcome/main menu, keypad ENTER, scroll,
stop and both transport selections were checked visibly. Bench startup VIN now succeeds through both transports; OEM engine codes and
returning from direct mode to OEM were also checked. full vehicle A/B
qualification remains pending; this is not a public Windows release.
See `releases/consistent-baseline/WINDOWS_AB_PREVIEW_2026-09-24.md`.

## Historical 0.2.3 behavior and packaging reference

The Windows shell follows the current Android firmware-first layout: connection and offline start, vehicle/status details, the original LCD with its aspect ratio preserved, S1–S4, EXIT, keypad, Actions, console and firmware selection. It uses .NET Framework supplied with Windows; users do not need a compiler, SDK or WiX. This remains an **x64 application**, including when Windows 11 ARM runs it through compatibility support.

The per-user installation is `%LOCALAPPDATA%\OpenSAAB\WindowsApp`. The single **OpenSAAB** shortcut opens the home screen. Firmware, per-session logs and reports live separately in `%LOCALAPPDATA%\OpenSAAB\WindowsData` and survive uninstalling. Existing bundled support files are imported once; an unrecognized old card is preserved and a verified catalog image can be selected instead.

## Adapter behavior

Chipsoft Pro uses the **native Chipsoft USB/CDC protocol** (`chipsoft-vin.exe`, the same Rust code as Android and macOS) over the `COMn` port that the inbox usbser class driver assigns; no vendor J2534 library is required or bundled. USB enumeration identifies a candidate; it is not proof of ECU communication. **Connect and start** requires a fresh, validated VIN and a clean channel/device teardown. The Trionic 8 HS-CAN engine DTC report still goes through the legacy `read_t8_dtc.ps1` path and is skipped with an explicit note when the vendor DLL is absent; its native port is pending. VCX Nano has no supported route in this Windows preview; its helper remains a comparison tool. Native helpers and the startup watchdog own the same exclusive adapter lock.

The HS-CAN action reads engine codes only. Security access, automatic engine-data navigation, code clearing, report upload and automatic app updates are not yet qualified in this shell. Manual original-firmware navigation remains available within the native bridge's allowed operations. These limitations prevent declaring complete Android feature parity or a globally qualified release.

## Firmware and interface language

`emulator/ui/firmware-catalog.json` is generated from the Android catalog, with its source checksum. It provides the same 11 pinned images / 10 runnable choices in nine languages. Russian 140.500 remains blocked. Archive size, archive SHA-256 and extracted image SHA-256 must all match before selection; the active selection changes atomically and each session uses a copy. Downloads can be cancelled and have a 60-second deadline. OpenSAAB controls currently remain English, as in the reference Android build.

```sh
python3 tools/ui/export_android_catalog.py /path/to/OpenSAAB-T2-languages --check
```

## Build

Private payload inputs:

- `tech2-emu.exe` built with `gui,load-test` (or a headless `load-test` build supporting the same mailbox protocol).
- `opensaab-reports.exe`, built from the same shared Rust source (`cargo build --release --bin opensaab-reports`).
- `chipsoft-vin.exe`, built from `emulator/src/bin/chipsoft-vin.rs` (see *Building the native helper* below).
- `vcx-native-bridge.exe` and `chipsoft-native-bridge.exe`, compiled as x86 from `emulator/scripts/windows/vcx_native_bridge.cs`; define `CHIPSOFT_PRO` only for the latter.
- `adapter-tools/invoke_vcx_t8_vin.ps1`, `adapter-tools/vcx_t8_vin.ps1`, and the Chipsoft bucket's `read_t8_identity.ps1` / `read_t8_dtc.ps1`.
- `LICENSE`, `THIRD_PARTY_NOTICES.md`, and the existing four private firmware files under `runtime`.

Do not commit or publicly distribute these private test payloads. The preview still bundles private support firmware; a production firmware-free installer is separate work.

Builder prerequisites: .NET SDK 8 for WiX, WiX CLI 5.0.2 with `WixToolset.UI.wixext/5.0.2`, Windows .NET Framework C# compiler, and the x64 Visual C++ runtime DLL from the build toolchain. Set `DOTNET_ROOT` if the SDK is installed outside the default location.

```powershell
.\build.ps1 -PayloadDirectory C:\private\payload -OutputDirectory C:\private\output `
  -WixExecutable C:\build\tools\wix.exe -VcRuntimeDll C:\build\redist\x64\vcruntime140.dll
```

The builder embeds the catalog from `emulator/ui`; a standalone build staging folder may place a copy next to `Launcher.cs`. Output: `OpenSAAB-0.2.15-x64.msi` and a SHA-256 payload manifest. MSI validation remains enabled. This is an unsigned developer preview. See the release notes for exact artifact checks and known limitations.

## Validation

Run the installed `OpenSAAB.exe --self-test` for checksum, unsupported-language, framebuffer, incomplete-report and cleanup-failure guards. It writes `shell-selftest-result.txt` and never opens an adapter. Test actual UI rendering, download, navigation and device behavior separately.

`test-lifecycle.ps1` checks install, file hashes, shortcuts, repair, uninstall/data retention and reinstall. Back up the installed app first and close all adapter sessions. Windows 11 ARM UI evidence and exact package checksums are recorded in [the shell validation](SHELL_VALIDATION_2026-09-20.md). Earlier 0.1.1 evidence remains in [the historical lifecycle report](VALIDATION.md) and [ARM smoke test](WINDOWS11_ARM_VALIDATION.md).

## Building the native helper

`chipsoft-vin.exe` is a plain Rust binary with no Windows-specific dependencies (std only;
the COM port is opened as `\\.\COMn`). Build it on any Windows machine with rustup:

```
rustup target add x86_64-pc-windows-msvc        # or aarch64-pc-windows-msvc on ARM hardware
cargo build --locked --release --features load-test --target x86_64-pc-windows-msvc --bin chipsoft-vin
copy target\x86_64-pc-windows-msvc\release\chipsoft-vin.exe <payload>\chipsoft-vin.exe
```

The x64 build runs under Windows 11 ARM emulation like the rest of the x64 payload. A native
`aarch64-pc-windows-msvc` build also works for bench use but must not be packaged into the x64
MSI without changing the payload manifest. Test standalone first:
`chipsoft-vin.exe --port COM5 --timeout-ms 8000` prints `VIN=…` and `MODEL_YEAR=…` on success.

## 0.2.11 recovery checkpoint

The shared Rust core now records adapter failures in `report.json`; the shell
shows that explanation and removes the stopped firmware frame. Live snapshot
readers, including the runtime harness, allow concurrent atomic file replacement
on Windows. A regression check holds a snapshot open while replacing it.

0.2.9 is superseded: its first installed package reused an old emulator binary,
and its later clean rebuild failed the OEM-menu runtime check. Do not identify
those artifacts by version text alone. Use the 0.2.11 receipt and payload hashes.
Direct Chipsoft remains the launch default; the vendor DLL route remains an
explicit experimental comparison. No public firmware-distribution policy changes
are made by this private bench build.

For a source-archive rebuild, clean this crate before compiling: archive timestamps must not allow Cargo to reuse an older emulator. The 0.2.11 bench build used `cargo clean --release -p tech2-emu`, `cargo clean -p tech2-emu`, `cargo build --locked --release --features load-test --bins`, and `cargo test --locked --features load-test --lib --bin tech2-emu`. Later shell-only packaging retained the verified Rust binary hashes. See the 0.2.11 receipt for exact inputs and qualification limits.
