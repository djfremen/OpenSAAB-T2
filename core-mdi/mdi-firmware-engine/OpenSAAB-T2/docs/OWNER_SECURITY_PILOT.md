# Owner security-access pilot — 2026-09-16

This ARM64 owner test adds operator-entered, expiring bearer authorization to the
OpenSAAB primary request only. It contains no server credential, vehicle grant,
security database or calculation handler. Existing local diagnostics remain
unchanged. ARM32 is not rebuilt for this pilot.

Open **Security access authorization** on the main screen to enter or remove the
private authorization. It is encrypted with Android Keystore in noBackupFilesDir;
backups are disabled. The server validates expiry, revocation and vehicle
entitlement independently. The APK does not confer authorization.

The processing confirmation describes the restricted owner pilot's raw 714-byte
input storage (including VIN/security data), deletion scheduled after one day,
seven-day outcome records, operator access/deletion contact and separate Bojer
fallback consent. No bearer is sent to Bojer, placed in request files or logged.
Unauthorized, expired or wrong-vehicle requests do not trigger fallback.

After processing, the existing SSA validation and atomic working-card import
remain mandatory. HTTP success and import are not proof of vehicle acceptance.
A fresh collection and owner-observed vehicle acceptance are required for each
vehicle. No vehicle test is implied by a software or historical fixture test.

This is a limited owner pilot, not public account sign-in or public security-access
availability. Keep the temporary private authorization out of screenshots,
support reports, chat, source and release artifacts. This build is installed
locally for the owner and is not advertised in the public release manifest.

## Persistent processing receipt (preview.6)

The Chipsoft security panel now preserves the latest local attempt across returning
to the firmware and process restarts. It records start, validated server reply,
card import and failure times in UTC, displays local time with timezone, and offers
request/provider details. These are device-clock observations, not server-signed
timestamps. The request number is retained only in its expected format.

The compact panel is shown only after the current session has identified the same
vehicle. A receipt from another session is explicitly previous processing, and a
stopped session or changed card cannot be represented as current authorization.
Only a SHA-256 vehicle identifier and SSA fingerprint are stored in the private,
non-backed-up receipt; no raw VIN, security keys, payload or bearer is included.
The existing private per-session result also receives the processing timestamps.
Fresh collected SSA must match the session's identified VIN before contacting the
service or modifying the card.

**Still outstanding:** automatic confirmation of the final vehicle/module response.
This build deliberately displays **Vehicle access: not verified**, even after a
successful API response and verified import. It never invents an access-granted
time. Capture the actual owner test's firmware/vehicle response before implementing
positive detection; seed responses, adapter TX acknowledgments, a menu label and
HTTP 200 are insufficient evidence. Grants can be module- and session-specific.

Validation: local JVM regression suites cover durable timestamps, vehicle/session/
card association, disconnect/failure and prevention of false grant claims. Android
instrumentation uses synthetic private data without contacting an API, opening USB
or importing firmware. Physical vehicle acceptance remains a separate test.
