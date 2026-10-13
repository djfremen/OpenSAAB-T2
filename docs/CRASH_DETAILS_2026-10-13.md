# Photo-readable failure details — October 13, 2026

Yesterday's private reporting candidates were Android 0.1.0-preview.65 and Intel
macOS 0.2.24-reporting.1. They correlate sessions and attempts, retain the first
failure, distinguish process exit from USB cleanup, and label missing evidence.
The public downloads were not replaced.

Their private successors are Android 0.1.0-preview.66 and Intel macOS
0.2.24-reporting.2. The same adapter_diagnostics.rs source is used in both.
Console, crash-diagnostics.json, crash.json and report.json carry a compact local
snapshot: packaged app version, architecture, build, connection UUID, adapter
family, observed model, firmware, hardware/driver versions when known, transport,
last observed state/phase, compatibility, session mode, last operation, USB and
protocol cleanup, terminal PC and process exit. Chipsoft identification is saved
before its existing Pro 1.5.2 compatibility check. This does not expand support.
Unknown values stay explicit; serials, raw identity, VIN and security material
are excluded from this summary. Android refreshes it after host cleanup, also
for the existing MDI engine. Existing reviewed support uploads retain their
current schema and include the validated firmware token and existing failure,
state and cleanup evidence. Full model/hardware snapshots remain local.

The installed names and manifests include the full version and architecture.
Android ARM64 explicitly adopts the program-identity rule. Its exact signed APK
was installed in a disposable API36 emulator; the installed hash matches the
artifact. Its main header visibly shows the full version and ARM64. The native
formatter and schema2 instrumentation pass without USB or firmware execution.
Android Rust tests: 256 passed, zero failed, ten ignored.

Intel package architecture, version metadata and ad-hoc signature pass. The
same-source ARM64 package passes 162 self-tests and the synthetic formatter.
Native Intel execution and live adapter/vehicle checks remain pending. The
broader Mac load-test run has 364 passes and one Nano shutdown-fixture mismatch;
the same mismatch reproduces in yesterday's unchanged source. No transport
sequence was changed to make that fixture pass. Other target adoption and full
cross-platform qualification remain pending.

Exact source, hashes and evidence are recorded in the companion JSON receipt.
Runtime changes remain in isolated branches; the canonical checkout records
this candidate without adopting or publishing its runtime.

## Publication — October 13, 2026

The exact preview.66 APK is now public at
https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.66 .
The Android updater and https://www.opensaab.com/static/t2/download.html serve
its full versioned filename and verified hash. Public APK, matching source,
receipt and checksum downloads were byte-verified. The signing certificate
matches preview.64. Physical qualification remains pending. The companion
Intel reporting.2 release is also public; Apple silicon and ARM32 keep their
separate releases. No runtime bytes were changed during publication.
