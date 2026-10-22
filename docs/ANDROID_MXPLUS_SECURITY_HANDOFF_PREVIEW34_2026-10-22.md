# Android ARM64 MX+ security handoff — preview.34

This distinct candidate follows the installed preview.33. Its new executable
identity is `0.1.1-preview.34`, version code `101034`; historical source,
artifacts, installed results and rejected preview.31 evidence remain unchanged.
Source/software checks, exact package identity, installed checks and physical
qualification require separate evidence for this version.

MX+ native shutdown now distinguishes protocol cleanup from transport ownership.
Only completed protocol cleanup can send the local `FINISH CLEAN` command.
Its single acknowledgement is `CLOSED` for ordinary Stop, or `PARKED` when the
host explicitly armed security processing. A protocol failure sends `CLOSE`
for full shutdown. An invalid or missing finish acknowledgement attempts full
shutdown and remains a failure even if that fallback confirms closure.

The transcript records `protocol_cleanup` (`succeeded`/`failed`) separately from
`transport_disposition` (`parked`/`closed`/`unknown`). `cleanup=confirmed` requires
both successful protocol cleanup and a valid acknowledgement. Parked ownership
does not assert Bluetooth release, a successful diagnostic operation, current
ECU authorization or a firmware/card lease. Final native outcome collection reads
the actual nested transcript events and preserves explicit cleanup failures;
later summary text cannot erase them. Existing runtime errors remain failures
even if the subsequent protocol cleanup succeeds.

The Android processing handoff must require an explicit arm, confirmed protocol
cleanup and an actually dead firmware child with exit code zero and no forced
termination. A separate owner may then hold the same SPP connection while the
working card is exclusively imported. The existing rebuilt connection worker
provides an adapter-only idle scope: serialized ATI checks with a closed protocol,
without a vehicle sender or retained ECU authorization. It must stop at a checked
prompt and actually exit before one-use transfer for fresh VIN discovery.
Run/card, imported-card hash, device/pairing compatibility and exact VIN stay
bound. The before-import hash is captured from the actual stopped working card,
under the firmware image lease after child death; it is not inherited from the
pre-run baseline. The parked hold expires after five minutes. Cancellation, backgrounding, expiry, import failure or uncertain ownership
closes the transport. No automatic retry or vehicle-operation replay is added.

Shared readiness recovery accepts only bounded, completed STOPPED/`?` monitor
residue, waits for a quiet period and tries one new nonempty AT. A noisy,
incomplete or repeated recovery fails before vehicle work. It does not replay
an unknown last command. SPP evidence records actual observed confirmed release,
connect start and first probe/vehicle writes on Android elapsedRealtime. Prior
release is unknown after process restart. Sanitized support evidence keeps the
MX+ adapter identity, child/cleanup/parked facts and timing without private raw
VIN, device or protocol data. These changes have their own source/software tests. They cannot by themselves fix or explain the earlier failures inside
Bluetooth socket connect, which occurred before any adapter write. Normal
close/reopen and measured sleep/reconnect remain separate physical tests.
ERAM POST failures and UART timeout evidence remain observable.

The tracked [build procedure](ANDROID_PREVIEW_BUILD.md) requires a clean committed
source and a new external output directory, planned as `local/session-safety/preview34`.
Four crate builds rebuild six packaged native entries: common root/MX engine
roles, recovered engine, MDI engine, MDI JNI/USB carrier and the shared connection
worker including its idle scope. The two unchanged Nano/Chipsoft probe entries
retain exact preview.1 hashes and original source provenance. The APK, all eight
native bytes, matching current and retained-component source, signing identity,
toolchain and dependency input receipts accompany the candidate manifest.
No independent byte-for-byte second rebuild is claimed without its own receipt.

No build, installed or physical pass is established by this source note. All four
adapters still require the exact candidate's common workflow and final cleanup
evidence. Other platforms remain pending separate bindings, packages, installed
checks and physical qualification. Internal authentication, authority profiles,
operation consent, original physical confirmations and validated SSA import
remain required; no operator password is introduced.
