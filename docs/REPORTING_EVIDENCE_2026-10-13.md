# Reporting evidence update — Android and Intel macOS

Sources begin with published Android preview.64 and immutable macOS 0.2.23-ui-parity.3. This is a reporting candidate, not a claim to repair the vehicle failures seen in earlier reports. Native Android adapter executables remain byte-for-byte identical to preview.64; see the native provenance receipt. MDI and Nano remain included.

## Implemented

- Fixed adapter identity and connection/session correlation. Unknown adapters no longer appear as Chipsoft or offline. The incident session is retained before unrelated recent sessions when its ID is available.
- Evidence schema 2 explicitly identifies missing/corrupt/oversize artifacts, records fixed operation/mode labels, and separates original failure from child-process exit, forced stop, USB release and protocol cleanup. No successful cleanup is inferred from a closed process alone.
- Terminal PC plus a bounded 32-address trail; shared fixed emulation-stage/CANdi classification and explicit performance sample retention/truncation. A terminal address may be a secondary fault and does not establish root cause.
- Android preserves its existing process-exit, lifecycle/health, privacy review and Java exception handling. Mac uses the existing shared Rust reporting worker and native review/send UI. No automatic upload or unrestricted raw log export.
- Server adds strict bounded evidence validation and generated session IDs, preserving Android, desktop and the live iOS client. Report receipts require confirmed persistent storage.

## Validation and limits

Rust reporting tests: 7 passed. Android existing seven instrumentation suites passed; evidence-specific fixtures also pass. These cover primary failure retention, identity, missing/corrupt evidence, privacy and execution-history limits. The Intel candidate cross-compiles on M4; native Intel execution is not implied. Final packaged and server test results belong in the artifact receipt alongside this note.

Next reproduction should use the new exact package, include the action being attempted and whether the failure repeated, then review/send the report. Missing artifacts from historical crashes cannot be reconstructed. Original bus payloads, VIN, seed/key material, firmware and raw memory remain local. Additional targeted private captures may still be needed to find the original guest fault.

Physical qualification remains pending: install the candidates on Intel macOS and Android, reproduce startup/no-adapter/cancel/disconnect and any affected read-only vehicle path, confirm the resulting report ID and correlate it to the exact build. Public rollout is not declared complete by compilation or emulator checks. Windows, Linux, ARM32 and Apple Silicon app releases are outside this pass.
