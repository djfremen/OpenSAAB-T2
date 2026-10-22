# Preview.33 audit follow-up

The installed preview.33 remains the exact `52ae248317aba9394bcce2ba9c9cda8d534dfaed`
artifact, APK SHA-256
`e2661e338f66e8f4ab70a9e6116706c9db36b97eef857a4c02a9c008c0b352c2`.
This follow-up corrects source comments and test guidance; it does not rebuild,
replace or qualify that executable. Any subsequent executable change needs a
new full preview version and its own retained components and verification.

The retained October 22 evidence confirms three failures inside Bluetooth socket
connect, before any vehicle write. Adapter AT/CR readiness begins after that
phase. Its success cannot establish a fix for those failures. The short
post-import gap must be tested separately from actual observed sleep. Record
Stop, actual cleanup/transport release where observable, import, Continue and
connect-start/outcome separately. Manual visible cleanup timing remains a proxy
until a future executable records release and connect on the same monotonic
clock, per physical adapter and Android boot; an unknown prior release stays
unknown rather than being inferred from Stop or a file timestamp.

The next owner instructions are retained locally as `START_TESTING.2.txt` beside
the immutable preview.33 artifacts. They add one controlled manual retry after
30 seconds following a failed, fully released attempt, while preserving both
results. They also require at least ten minutes plus an observed sleep indication,
constant ignition conditions and explicit recording of any voltage/ignition wake.
No timing-only test establishes the underlying Bluetooth cause. Required physical
prompts and vehicle-change confirmations remain in force.

A force stop leaving STM active is a plausible recovery case, not a guaranteed
failed next connection or a captured physical result. FRPM F section 8.9 says a
character can terminate monitoring, with STOPPED/prompt and that character
discarded; the next response depends on the remaining bytes and adapter state.
Preview.33 deliberately fails before vehicle requests if readiness is ambiguous.
A future recovery implementation needs a bounded adapter-only resynchronization
fixture for residual STM, partial input, STOPPED, rejected AT and excess prompts;
it must never repeat an unknown prior vehicle command or hide the failed attempt.

## Security-processing transport retention

This remains an open implementation task. `NativeTransportLease` is a one-use
discovery-to-firmware transfer. Current native cleanup sends CLOSE, expects
CLOSED, and the Java byte bridge shuts down Bluetooth both on CLOSE and in its
finally block. Security processing waits for combined native/transport cleanup
before taking the exclusive working-card change lease. Reusing that discovery
lease alone cannot retain a socket through processing.

The intended next change must distinguish protocol cleanup, stopped firmware,
exclusive card access and retained Bluetooth ownership. The native engine must
actually exit before import, while a dedicated transport owner can retain the
same socket only after verified protocol cleanup. Ordinary Stop must still close
everything. A processing handoff needs explicit cancellation, background/expiry
cleanup, import-failure handling and one-use resumption bound to the same device,
pairing choice, run/card and freshly checked VIN. It may keep the adapter UART
alive only through serialized benign adapter traffic while the protocol is
closed; it must not preserve ECU authorization, synthesize vehicle Tester Present,
replay an operation or remove original physical confirmations.

Required software cases include native failure, forced termination, unconfirmed
cleanup, cancellation during API/import, expired or stale handoff, wrong card/VIN,
backgrounding, repeated Continue and ordinary Stop. Package/installed checks and
physical import/continuation must use the next exact version. Retention removes
the post-import socket open; fresh initial and normal close/reopen behavior still
need their own tests. No retained-link implementation or physical pass is claimed
by this document.

## Findings classification

The six new-connection shortcuts are guarded by `!running` in MainActivity's
Actions sheet, so B5 is traced in source. Root/MX native files intentionally use
the same common-engine build under two role names; their byte identity is not a
mismatched-engine bug. Eliminating duplicate packaging is a separate size/task
decision. The two retained preview.1 probes remain explicit maintenance debt;
their original hashes/source are retained separately, while the six other native
entries are rebuilt. Neither point establishes hardware qualification. ERAM POST
failure remains an open observed issue and must not be changed into a synthetic
Pass.

Primary reference: [OBDLink FRPM F](https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf),
sections 8.9, 15.2.2, 15.6.8 and 15.8. Ten minutes is the default UART inactivity
timer; the manual also documents CAN sleep inhibition and other wake triggers.
The contributor advice is checked against the owner's quoted text; searches of
the connected mailboxes did not locate the original email.
