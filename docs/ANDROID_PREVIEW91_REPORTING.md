# Android preview.91 reporting

This ARM64 preview adds the last observed Rust panic's relative source file,
line, column and bounded thread name to the existing reviewed support report.
Panic messages and raw log text remain excluded. Upload still requires the
existing report review, affirmative consent and explicit Send action.

Offline emulator and VCX Nano process logs retain panic headers after their
2 MiB routine-output cap. Both use the same retention predicate. The report
collector continues to read only the bounded tail and preserves existing event
counts and failure categories. The support instrumentation covers both capped
log names, source-path shortening and exclusion of a private-message canary.

Deploy the additive support API validation before distributing this APK. The
API accepts exactly file, line, column and thread inside panic_location, rejects
messages, absolute/traversing/deep paths and malformed values, and retains the
existing Android, desktop and iOS schemas.

Grouping, optional model classification and report catalogs run in the private
administration environment. They are separate from the Android package. This
change does not claim a crash fix or additional adapter compatibility. Exact
installed-package reporting and security-password evidence, server rollout and
physical regression scope must be retained with the release candidate.
