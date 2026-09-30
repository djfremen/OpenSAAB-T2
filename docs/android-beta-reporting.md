# Android limited-beta diagnostic evidence

This is a diagnostic candidate rebased onto installed preview.24 source `902c46f`, not a release qualification or a fix for the submitted incidents. It changes shared Android reporting; both architectures still require release qualification independently.

## Changes

- Log summaries retain up to 16 fixed failure categories and relative line positions from the existing bounded tail. Raw log text, paths, VINs, tokens and transport payloads remain excluded. Unknown errors still require additional investigation; this is not arbitrary-text redaction.
- Categories distinguish adapter firmware validation, adapter status rejection, USB open/claim/read/write failures, local bridge failure, missing firmware and time limits.
- Session diagnostics record at most 32 timestamped events: start, pause/resume, expected stop, unexpected exit, security collection/processing request, import and requested restart. Restart requested is not proof the firmware restarted.
- Chipsoft errors and health incidents carry a random session identifier; error-to-incident association is explicitly same-session, different-session or unknown. This does not establish causal ordering within a session.
- Health incidents include start/end orientation and pause/resume counts. These counters are evidence for lifecycle investigation, not proof of an orientation defect.
- Report review offers vehicle/bench/offline context and optional follow-up contact with an unchecked consent checkbox. Contact is included in the existing user-provided description only when consent is checked. Descriptions reserve space for that context within the existing 2000-character limit. Inputs survive activity recreation. Nothing uploads automatically.
- Known vehicle-network failure events now survive the support-report health allowlist.

## Investigation order

1. Early native startup: compare identification and native startup paths before changing adapter policy. Source currently accepts a broader Chipsoft Pro identification during probing, but the native backend checks a specific firmware prefix. A mismatch is a hypothesis until an affected session records `adapter_firmware_not_validated`. Do not remove validation speculatively.
2. Security handoff: trace processing request, expected stop, import, restart request and next session start. Existing expected-stop suppression passes the synthetic test; that does not explain every field incident.
3. Ignition-cycle recovery: reproduce the read-only sequence on a bench where applicable, then a vehicle. HS-CAN-only benches do not qualify single-wire network workflows.
4. Unclassified exits: preserve as separate cases until evidence supports grouping.

## Tests

Run `python3 scripts/android/test-request-gate.py` for existing host regression checks.

Run `python3 scripts/android/test-beta-reporting.py --serial emulator-5554` on a disposable AVD. It builds a separate `com.opensaab.beta.diagnostics` harness and removes it afterward. The harness contains no native emulator or firmware and is not a distributable application. It tests report privacy, bounded classification/events, contact consent, size limits, review/export, health timing, expected stops and report prompts without live network uploads or vehicle commands.

API 36 ARM64 reporting/health instrumentation and host checks passed during this change. The release-signed private preview.25 APK (source `2fc2b4d`, SHA-256 `193668a1b034d2da49ac0aac435d55f5223a28b13eba050230d9ee7353ba66b1`) upgraded preview.24 on the owner Pixel 7. Native libraries were reused byte-for-byte from that installed baseline. On a Chipsoft Pro 1.5.2 / Trionic 8 HS-CAN-only bench, both versions returned the same six stored codes and status bytes. The candidate reached Main Menu on two firmware starts and stopped normally. No code clearing or security processing was attempted. These are owner bench results, not evidence from an affected field device.

The live service rejected the candidate report. Its existing field allowlist does not accept `session_id`, `diagnostic_events` or the new failure/lifecycle fields. The reviewed JSON is retained privately. The server compatibility patch adds only the new keys, validates fixed diagnostic labels and enforces event/category bounds. Ten synthetic service tests pass, and the actual Pixel JSON round-trips intact through the patched local service using mock storage. The companion desktop-capable service preserves desktop handling and passes eleven tests. The server patch was subsequently deployed on 29 September Pacific time, preserving the current image configuration and changing only the report validator. An actual Pixel preview.26 reviewed owner-bench report uploaded successfully; independent R2 retrieval matched the JSON exactly, including firmware 1.5.2. This closes the live-schema/upload blocker, not the original crash incidents.

API 28 and 31 clean installation, offline native startup and reporting lifecycle checks are recorded below. Security-handoff qualification, affected-model retests and 32-bit physical qualification remain pending. Do not mark the user incidents fixed or publish a production APK on these tests alone.

## Triage and publication

Maintain private receipt-to-incident mapping outside the repository. Deduplicate historical sessions and keep observations separate from hypotheses. Progress states: open → investigating → reproduced → fix candidate → verified → released. Each case needs an exact tested artifact/version, reproduction steps, next test and completion criteria. Notifications should cover actionable changes or access failure; unchanged polls stay quiet.

Public updates contain aggregate models/builds, broad symptom groups and testing limitations. Exclude receipt numbers, submission times, contact details, VINs, security data and raw descriptions/logs. Seven reports are not seven unique devices and cannot establish a failure rate.

## Adapter firmware identity

The Android Chipsoft view observes the existing GET_INFO exchange without adding USB requests or vehicle commands. A bounded observer checks framing, status, checksum and version syntax, including fragmented replies. Connection details show the reported version, even if native startup subsequently rejects it. Stopped sessions label it last detected; each new discovery clears the prior identity. Unknown or missing replies remain not detected. Version detection is not a compatibility claim. Reviewed session reports include only the validated version token, never the raw identity or adapter serial. Server allowlist deployment and actual Pixel preview.26 upload/retrieval have been verified.

## Android Studio lifecycle checks

On the local API36 ARM64 AVD, report-screen landscape/portrait rotation, explicit Activity recreation and Home/resume preserve the typed description, contact draft, testing context and unchecked contact consent. These are real Activity transitions with synthetic input; they do not run the native emulator or contact the network. Existing report privacy and health-monitor tests passed in the same run. This is not evidence for the reported Motorola native startup failure.

The existing OpenSAAB_SetupTest profile is 1024×600 at 160dpi. It did not honor the requested portrait transition in this run. The phone-profile run used temporary 1080×2400 at 420dpi overrides and passed; overrides were reset afterward. Add `--lifecycle` to the harness command on a phone-sized AVD:

```sh
python3 scripts/android/test-beta-reporting.py --serial emulator-5554 --lifecycle
```

API28, API31 and API36 are now installed locally. Actual low-memory process death/relaunch, native firmware rotation and field-device USB behavior remain separate pending checks. Synthetic health-state tests are not a substitute for those cases.


## Android 9 and 12 offline qualification — 29 September 2026

Google APIs ARM64 AVDs running Android 9 (API28) and Android 12 (API31), each configured with 2 GiB RAM and a 1080×2340 phone display, were clean-installed with the unchanged release-signed private preview.26 APK. APK SHA-256: `fa7e7bf98c1eb7a24df258c1f373543146d76579f90b73d7a383eb69e11629a8` (application source `8dbd9f7`). Both downloaded and prepared Saab NAO 9.250 English through the normal setup flow.

Both passed:

- Actual offline firmware startup, Main Menu, keypad input and Model Year navigation.
- Workspace layout instrumentation covering phone and landscape bounds, expanded controls, EXIT, Actions, menu and status.
- Report privacy/validation, fixed failure taxonomy, passive identity parser and synthetic health-monitor checks.
- Actual report-form portrait/landscape rotation, Activity recreation and Home/resume with draft/context retained and contact consent remaining unchecked.
- Deliberate `am crash` of the installed app with offline firmware running: next launch offered the report, the reviewed JSON retained the application crash breadcrumb and session events, and no native process remained. Reports were kept privately, never uploaded. API31 additionally supplied Android process-exit reason 4; API28 correctly omitted the unavailable Android exit-history field.

The first API31 health test inspected the report view too early and failed `Report review skipped`. Waiting for UI idleness and checking the view on the main thread corrected this test synchronization defect; the complete harness then passed on both versions. No application behavior or APK was changed.

A brief Home-and-return on API31 retained the process, but **full backgrounding ends offline emulation on both versions**. `MainActivity.onStop()` explicitly requests this stop. The API28 report recorded `PAUSED` followed by `EXPECTED_STOP`; restarting offline emulation worked. A quick switch is therefore not evidence of background session persistence. Do not count the expected background stop as a reproduced field crash.

These tests do not qualify Android 8, ARM32 head units, 1 GiB hardware, real memory exhaustion, vendor-specific Android behavior, USB transport or vehicle/security operations. Existing field incidents remain open. The AVDs were shut down after testing and retained for repeatable follow-up; the physical Pixel was untouched in this round.
