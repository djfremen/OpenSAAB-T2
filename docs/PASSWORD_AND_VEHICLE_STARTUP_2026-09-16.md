# Password and vehicle startup changes — 2026-09-16

Prepared locally; not deployed or published. The live owner pilot remains separate
until the operator resolves whether the shared password covers all supported
vehicles or only the two existing test vehicles.

The Android service-password dialog replaces manual expiring owner-token entry.
It preserves case, rejects whitespace rather than silently trimming it, stores the
entered value with Android Keystore encryption, and resumes the processing consent
flow after saving. The server checks the password. A primary HTTP 401 clears the
saved value so Retry asks again. OpenSAAB credentials are never sent to Bojer.
Existing owner credentials are not silently converted to the shared password.

Before VIN discovery, Chipsoft startup explicitly asks for ignition ON, with
instrument lights on; ACC is insufficient and the engine need not run. After a
fresh VIN and online lookup, a review dialog shows VIN, vehicle, engine and color.
The user presses Start firmware to proceed. Failed online enrichment is explicitly
shown as unavailable; the fresh VIN can still be used for offline diagnostics.
After startup the firmware's own key-position instructions take precedence.

The private backend follow-up provides an allowlisted metadata response, with no
security codes or legacy database response fields. It keeps legacy calculation,
admin and file routes closed. Processing logs contain operation, opaque call ID,
HTTP status and duration, never raw VIN/password/SSA. Raw input storage remains
under its separate private processing disclosure and retention policy.

Local verification: JVM regression suites; Android startup and security-screen
instrumentation; 46 backend tests; 21 checks of the built private container. A
read-only metadata lookup for the owner's reference vehicle returned the expected
engine and color through the restored decoder. No fresh vehicle security transaction
or production deployment is implied by these checks.
