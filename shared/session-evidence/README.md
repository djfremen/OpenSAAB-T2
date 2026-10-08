# Shared offline session evidence

This small Rust crate gives the Android engine and canonical desktop engine one
fixed vocabulary for termination and screen stages, plus one bounded resource
sample policy. It classifies observations; it does not infer a cause from an
incomplete exit, absent adapter, stale display or missing measurement.

`snapshot` exports fixed labels and numerical CANdi progress. Unsupported
accesses include the emulated address, width and read/write direction. The raw
reason, register value, display text, firmware contents, vehicle identifiers and
transport payloads are excluded. Unknown reasons remain `unknown`.

`PerformanceWindow` keeps the first two and last ten samples. Its maximum frame
age and minimum available RAM cover every sampled observation, not the intervals
between samples. Missing measurements stay absent. Storage remains bounded to
twelve samples and two extrema.

Android imports this crate from the canonical companion checkout at
`../OpenSAAB/shared/session-evidence`. Keep that checkout beside the Android
repository; its Cargo lockfile records the dependency. This is an explicit local
source dependency, not a vendored second implementation. Repository revisions
must be pinned together before distribution.

Run `cargo test --locked --manifest-path shared/session-evidence/Cargo.toml` from
the canonical repository. The Android host binding, uploader allowlist and server
privacy corpus are tracked in the Android repository. See
`releases/consistent-baseline/OFFLINE_REPORT_EVIDENCE_2026-10-02.md` for platform
qualification gaps and the candidate receipt.
