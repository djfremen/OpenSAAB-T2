# Android MCP working-card reset — 0.1.1-preview.28

The visible Reset to INIT_AUTH action now shares its transaction with an
explicit authenticated MCP action. Inspect the stopped working card first;
supply its SHA-256, manual generation, a unique request ID and the literal
RESET_TO_INIT_AUTH confirmation. The request is asynchronous. Poll the same ID,
read its result/backup hash, then inspect the card again for INIT_AUTH and the
same working-card hash. No file path, offset, arbitrary payload or vehicle
command is accepted through this control.

The native payloads are unchanged from the retained preview.26/27 ARM64 APK.
The 32MiB card backup, 714-byte security reset at FE0000, outside-region
verification and atomic replacement use the same shared Java implementation as
the UI. Progress is visible. The firmware lease rejects sessions during cleanup.
Foreground/owner/manual-generation changes or cancellation prevent a pending
commit; a completed commit is reported as changed rather than cancelled.

Request IDs and results are retained on the device. A repeated ID retrieves its
outcome, including after restart. A request interrupted before durable completion
is unverified and is never automatically replayed. If the app restarts, inspect
its card and retained reset evidence before deciding on a new operation.

First accept the app's Developer Preview welcome and pair the local MCP bridge.
Physical ignition and remaining vehicle/security dialogs keep their existing
operator semantics. An INIT_AUTH card state does not establish ECU authorization.

New endpoint/transaction instrumentation is restricted to a disposable emulator.
Source compilation, APK identity/install and physical Fire reset checks are
recorded separately. No release or vehicle workflow is qualified by source alone.
ARM32 and other platform bindings/builds require their own adoption and checks.
