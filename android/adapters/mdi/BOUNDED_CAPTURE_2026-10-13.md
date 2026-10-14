# Bounded MDI recording — Android preview.70

The original 128 MiB capture boundary ended a healthy native diagnostic session.
The shared Rust `opensaab-session-evidence::bounded_capture` recorder now skips
empty polls and retains at most 128 MiB of initial nonempty raw activity. Reaching
capacity, or a later recording I/O failure, does not become a transport error.
`mdi-cleanup-private.json` records omitted records, recording errors, real transport
failure, pending tickets and cleanup separately. Truncation is explicit; missing
raw records cannot establish absence of an operation.

The companion matching-source archive contains the exact shared recorder and
changed MDI firmware engine. `firmware-capture-preview70.patch` is the call-site
change against public preview.66 matching source. The packaged MDI USB module
retains its preview.66 bytes; only the native firmware bridge changes. Android
ARM64 software qualification does not establish a physical adapter pass or enable
MDI on ARM32/desktop. Canonical shared adoption is recorded separately from those
transport bindings. No adapter firmware update or diagnostic command policy change.
