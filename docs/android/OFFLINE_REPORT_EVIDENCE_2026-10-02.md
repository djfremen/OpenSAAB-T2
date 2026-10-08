# Offline emulation report evidence

The new Android candidate adds the evidence needed to distinguish an emulator
failure from an intentional or background stop. The reported failures came from
preview.29, not the current preview.36 source. They do not establish that running
without an adapter caused the exits. Offline emulation is an intended workflow.

## What the earlier reports establish

Two private submissions contain no submitter notes. Their description is a
health prompt with an empty activity explanation and selected offline context.
The retained summaries include three distinct unexpected incomplete exits and
older expected stops. One unexpected session appears in both reports and must
be deduplicated. The unexpected exits share a guest program counter, but the
uploaded records omit the native reason. Exit 3 means incomplete, not a proven
CPU crash. Available memory samples do not establish an out-of-memory cause.
The app version and retained historical Android exit events must not be treated
as evidence for today's binary or assigned to a different session.

## Omitted evidence and the implementation

| Earlier gap | New candidate evidence | Diagnostic limit |
| --- | --- | --- |
| Offline folders lacked exported session IDs | Stable offline UUID association in sessions, health warnings and app errors | Earlier reports cannot be retroactively linked with certainty |
| Native reason and CANdi termination discarded | Shared fixed reason and screen-stage labels; CANdi PC, cycles, progress, UART counts and fixed stop reason | Screen stage is an observation, not a causal conclusion |
| Unsupported emulated hardware access lost | Address, width and read/write direction only | Register values and source reason remain private and excluded |
| Actual engine and firmware identities unknown | SHA256 of executable and four firmware inputs under the firmware lease | Hashes identify inputs; they do not prove firmware compatibility |
| Host stop and forced shutdown ambiguous | User/background stop intent, host deadline, startup failure, forced-stop flag, exit and elapsed time | Missing terminal record is unavailable, not proof of normal completion or crash |
| Interaction and display timing incomplete | Accepted input count, changed-frame count and last elapsed timestamps | No key values, ordered keypad history or screen contents |
| Only final resource samples retained | First two and last ten samples, full sampled extrema and truncation flag | Five-second sampling misses events between observations; file freshness is not a responsive guest |
| Generated text confused with notes | Explicit boolean from the raw description field before context/contact assembly | Legacy reports require private review of the description |

Collection stays bounded. Reports remain frozen for review, require consent and
explicit Send, and include no raw traffic, vehicle identifiers, private codes,
register values, firmware bodies or screenshots. An absent crash artifact does
not exclude Android killing the native process or a failure before report output.
There is no new automatic upload, restart, bus command or raw retention extension.

## Source paths and qualification

The canonical shared crate is `shared/session-evidence`. Android's engine imports
that companion source rather than duplicating classification and retention rules.
`android/shared/com/opensaab/usb/OfflineSessionEvidence.java` records OS facts;
`MainActivity`, `SupportReports`, `EmulatorHealthMonitor` and report preparation
wire them into the ordinary offline/report workflow. Both native engines emit
`emulation_evidence`. Desktop/iOS collector forwarding and corresponding host
facts remain unfinished platform migrations; this is not a uniform release.

Candidate Android version is private debug preview.37 (100037), based on today's
dirty preview.36 worktree with these additions. The exact APK and workspace source hashes,
checks and remaining gates are in `OFFLINE_REPORT_EVIDENCE_2026-10-02.json`.
No signed public download or physical handset has been updated by this pass.
Source hashes describe the concurrent dirty workspace at receipt time; they do
not substitute for a clean coupled rebuild before distribution. The native hash
observed by the installed run matches the executable inside the tested APK.

Three shared Rust tests cover reason classification, privacy and first/last
retention with a mid-session extremum. Three native resource tests and the native
budget/fault/success outcome test pass. Gradle build, APK verification and lint
pass. ARM32 compile check passes; that is not installed ARM32 qualification.

On a disposable API31 ARM64 emulator, synthetic collection tests exercise exact
hashes, stop intent, offline correlation, fixed vocabulary, no-notes reporting,
privacy, and missing legacy evidence. An actual packaged offline run uses
original firmware, accepts input, publishes a frame and records explicit Stop.
This short run qualifies telemetry wiring only, not all menus or the original
failure. Existing reporting, health, receipt, rotation, LCD and session-end
regression suites pass without a vehicle connection or live upload.

The exact reports collected on Android validate locally against Android,
canonical desktop and an iOS-compatible service overlay. The privacy corpus
rejects raw reason/screens/payloads, malformed hashes and invalid flags/types.
All thirteen existing iOS service compatibility tests also pass against the
additive overlay. The overlay preserves existing iOS support; replacing production with the older
Android-only validator would regress it. Deployment is not performed here.

## Remaining release gates

Apply and verify the additive schema patch against the actual deployed service,
then qualify one explicitly consented candidate upload and its stored body.
Package and sign the release, verify upgrade preservation, and test this exact
build on the affected Android device and ARM32 before claiming those passes.
Keep the two repository revisions pinned together for the shared source dependency.

Reproduce the affected offline menu path once an actual description or new
telemetry is available. Compare native and host reasons, firmware fingerprints,
CANdi unsupported accesses and sampled resource bounds; do not guess from the
missing adapter. If the failure precedes terminal output, retain that evidence
gap and correlate the Android process-exit timestamp instead of declaring a cause.
