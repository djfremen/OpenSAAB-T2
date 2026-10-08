# Android report/session first pass — 1 October 2026

Review branch: `codex/android-report-first-pass`, based on `a800c6854ae362378c702313e02c6a4634d2325b`.
This is the independent first phase of the owner-approved reporting/session contract.
The Java collector and existing active-report guard remain. No collector worker,
server compatibility dependency, server deduplication or active-session reporting
is introduced. Publication checkpoint: ARM64 preview.29 was released on 1 October 2026 after owner authorization and the exact-package Pixel checks below. ARM32 remains unchanged.

## Review map

- `ReportReviewDialog.java`: AndroidX DialogFragment restores its window through
  rotation; configuration destruction is distinct from Keep private/dismissal.
  The bounded JSON preview scrolls above the consent switch and visible footer.
- `ReportReviewModel.java`: Activity-scoped AndroidViewModel owns the exact open
  artifact and consent. No affirmative upload consent is in Bundle, SavedStateHandle,
  preferences, files or checkbox hierarchy state. A new process starts unchecked.
  Dismissal/new preparation/invalid local bytes clear consent. Work uses Application,
  never a retained Activity; callbacks update the current lifecycle binding.
- `ReportArtifacts.java`: strict private UUID artifact resolution and bounded,
  strict UTF-8 `diagnostics.json` extraction. Review, restore and upload never
  parse/reformat the body. A receipt is bound to the saved filename plus SHA-256
  of those JSON bytes. ZIP metadata can change without changing this identity.
  Sent reports restore as Already sent with Send/consent hidden. Legacy receipts
  cannot safely be matched retrospectively. Retention removes matching markers.
- `SupportReportActivity.java`: draft fields and exact reviewed ID/hash/open state
  restore separately. It displays the review dialog for the existing stopped-session
  report flow. It is not yet an in-session overlay on the emulator Activity.
- `SupportReports.java` / `SupportUpload.java`: freeze once, save/export ZIP, send
  the same byte array through the existing bounded HTTPS contract. A local marker
  is recorded only if the reviewed file still matches. Confirmed delivery remains
  visible if the marker write fails, with a warning to retain the receipt.
- `NativeLcdPump.java`: generation invalidation discards queued/late frames from
  ended sessions, including restarts using the same directory. Evidence on disk
  is preserved. Main, Chipsoft and Nano terminal bindings clear live presentation;
  expected ends do not produce an unexpected-exit health incident.
- `MainActivity.java`: offline ECU information entry restored; missing adapter/fresh
  vehicle prerequisites described accurately. Late console callbacks cannot overwrite
  an ended/new session. EXIT remains guest navigation, not Stop.

AndroidX Fragment 1.6.2 / Lifecycle ViewModel 2.6.2 are pinned in Gradle. Both
Android Studio and SDK-only APK/test builds resolve the same dependency graph.
SDK packaging now uses AAPT2 and includes library resource classes and all DEX files.
The APK carries Apache 2.0 notices. Native emulator/probe binaries are unchanged.

## Validation and scope

See the accompanying receipt for final source, APK and evidence hashes.
The isolated instrumentation APK uses a separate disposable package, synthetic
reports/receipts and fake HTTP responses; it never opens USB or uploads to production.
Report tests cover exact UTF-8 bytes, ZIP repacking, altered artifacts, consent gates,
real portrait/landscape recreation, footer visibility, dismissal/reopen, cold sent
restoration, new preparation and marker pruning. The external process test performs
HOME → background process death → a verified new process and checks exact reviewed content/id/hash,
unchecked consent and disabled Send. The test requests `am kill`; when Android retains a cached process, it uses
SIGKILL under the disposable debug app’s own UID after checking it is background.
Force-stop is fixture cleanup only, not the process-loss operation being qualified.

LCD tests cover atomic publications, coalescing, malformed frames, directory switch,
queued/late frames after clear, same-directory restart and closing. Session-end
instrumentation exercises terminal presentation bindings with synthetic states;
it does not establish real USB cleanup or hardware release. Installed firmware
checks are recorded separately from those synthetic binding tests.

## Remaining gates

- Android ARM32 installed/runtime: pending; ARM64 evidence cannot be inherited.
  Physical Pixel 7 ARM64 evidence is scoped below.
- Forced adapter disconnect/failure and native original-firmware Stop/USB cleanup:
  pending hardware. The bounded HS-CAN reader reopened Chipsoft on Pixel 7; this
  does not qualify all adapter/session teardown paths.
- Shared Rust collector/formatter/artifact/receipt engine and Java-reference corpus:
  pending, followed by worker packaging per ABI and server acceptance checks.
- Reporting during emulation: remains blocked until that snapshot/schema migration
  and explicit input gate are qualified. No report-induced lifecycle pass is claimed.
- Lost server response: identical-body server deduplication remains separate;
  explicit retry can still create a duplicate when delivery was not confirmed.
- Confirmed response but failed local marker write: warning remains; after restart
  the same report may be offered again. No persistence success is fabricated.

The canonical contract lives in the companion OpenSAAB repository. Its scoped
adoption matrix retains historical iOS build8/desktop receipts; this pass does not
rebuild iOS, qualify desktop targets or declare uniform release conformance.

## Final review checkpoint

Implementation starts at `9a65a27`; packaging/process-test correction is `1c59e09`;
final application source is `fef93a72eb4db9ad5f9fd8c0f84b8a7e1722d3e5`. Review the
full branch against the pinned base, not only the final small correction. Fake
reply tests cover lost/failed responses and confirmed body-bound delivery; failed
responses never claim Already sent. End reasons remain in the workspace header
even with saved security history.

API31 and API36: seven instrumentation suites, actual background process-loss
checks and pure failure/identity checks passed. API36 retained the cached process
after am kill, so its isolated process test used app-UID SIGKILL. Both build paths
and the payload gate passed; Kotlin runtime resources now match, with hashes pinned
without broadly allowing arbitrary binary payloads.

The exact signed ARM64 preview.29 candidate was installed and pulled back unchanged
on API31: original splash/Main Menu/Diagnostics/model year/EXIT, offline ECU entry
and prerequisite message, real Stop, controlled native-child exit, report rotation
and actual cold process restoration passed. Stop/exit screenshots were inspected:
the live display is blank and the reason remains visible. Downloaded software was
retained through preview.28 and intermediate private preview.29 revisions; this is
not a fresh-install or direct final-package upgrade qualification.

`REPORT_REVIEW_FIRST_PASS_RECEIPT_2026-10-01.json` records exact artifact and private
evidence hashes. Private logs/screenshots stay in the owner's evidence directory;
no raw report body, firmware, credentials or private capture is committed. The
ARM64 candidate was subsequently authorized and published as preview.29 after
the physical checkpoint below; remaining gates retain their stated limits.

## Physical Pixel 7 checkpoint — 1 October 2026

The final signed preview.29 APK was installed in place over
preview.28-owner-audit.1 on a charging Pixel 7 running Android 16/API36. App data
was retained and the previous private owner-audit APK was preserved. Preview.29
does not include that private JEV audit instrumentation.

Original firmware reached Main Menu using long-press ENTER. Offline Actions
showed ECU information; Read DTC displayed the adapter/fresh-vehicle prerequisite
without ending offline emulation. Stop cleared the display and retained the
stopped reason even with existing security history.

The real report dialog remained open through portrait → landscape → portrait,
retaining consent with a visible Send footer. HOME → force-stop → cold relaunch
restored identical on-screen report JSON in a different process, unchecked consent
and disabled Send. Dismissal/reopen left consent unchecked. Send was never clicked.
This is a force-stop cold-relaunch check, not an actual OS memory-pressure process
loss test. Internal saved IDs and exact uploaded bytes were not independently read
through the release app sandbox; those checks remain the API31/API36 isolated tests.

With Chipsoft connected to the HS-CAN bench ECM, two independent startup VIN and
Trionic 8 code reads completed. Fresh identity was MY2004, adapter firmware displayed
1.5.2, and both reads returned the same six code/state/failure/status records as the
September 29 Pixel bench receipt. Both cleanup records reported success; the second
read reopened USB and no native emulator/probe helpers remained afterward. Optional
online vehicle lookup was disabled; no codes were cleared or security data processed.

The historical Android record is not a fresh OEM J2534 A/B. Original-menu ECU
information, SW-CAN, real-vehicle continuation, forced adapter failure, ARM32 and
active-session reporting remain unqualified. The private evidence hashes in the
receipt cover screenshots, result XML and local logs; their bodies are not committed.
Public preview.28 and production services remain unchanged.
