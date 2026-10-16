# Android preview.92 reporting

This candidate completes the reporting change described in
[preview.91](ANDROID_PREVIEW91_REPORTING.md). A real Rust 1.98 panic revealed
that the default hook inserts a numeric thread ID between the thread name and
`panicked at`. The parser now accepts both that format and the earlier format.
The numeric ID is not exported.

The already built preview.91 artifact is retained without replacement. The
changed parser receives preview.92 and its own installed-package evidence.

`RustPanicInstrumentedTest` runs an isolated Rust child packaged only in its
test helper. It emits more than 2 MiB of routine output before panicking. The
installed production collector must retain the real source location, exclude
the panic-message canary and build prefix, and prepare the existing structured
report. The test never opens an adapter or uploads a report. Server rollout,
reviewed synthetic upload/readback and physical adapter qualification are
separate recorded checks.
