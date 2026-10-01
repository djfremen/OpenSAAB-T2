# Android report/session first pass — 1 October 2026

Review branch: `codex/android-report-first-pass`, based on `a800c6854ae362378c702313e02c6a4634d2325b`.
This is the independent first phase of the owner-approved reporting/session contract.
The Java collector and existing active-report guard remain. No collector worker,
server compatibility dependency, server deduplication or active-session reporting
is introduced. Public preview.28 remains unchanged until separate publication.

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

- Android ARM32 installed/runtime and physical hardware qualification: pending;
  the ARM64 emulator evidence cannot be inherited.
- Real adapter failures/disconnect/reopen: pending hardware. Presentation tests
  do not prove port ownership release or successful ECU operations.
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
