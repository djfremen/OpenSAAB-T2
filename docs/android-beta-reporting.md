# Android limited-beta diagnostic evidence

This is a diagnostic candidate based on `29e7809`, not a release qualification or a fix for the submitted incidents. It changes shared Android reporting; both architectures still require release qualification independently.

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

API 36 ARM64 reporting/health instrumentation and host checks passed during this change. API 31, actual rotation/background transitions, clean/upgrade native firmware startup, live upload acceptance, Pixel/Chipsoft bench operation, affected-model retests and 32-bit physical qualification remain pending. Do not mark the user incidents fixed or publish a production APK on these tests alone.

## Triage and publication

Maintain private receipt-to-incident mapping outside the repository. Deduplicate historical sessions and keep observations separate from hypotheses. Progress states: open → investigating → reproduced → fix candidate → verified → released. Each case needs an exact tested artifact/version, reproduction steps, next test and completion criteria. Notifications should cover actionable changes or access failure; unchanged polls stay quiet.

Public updates contain aggregate models/builds, broad symptom groups and testing limitations. Exclude receipt numbers, submission times, contact details, VINs, security data and raw descriptions/logs. Seven reports are not seven unique devices and cannot establish a failure rate.
