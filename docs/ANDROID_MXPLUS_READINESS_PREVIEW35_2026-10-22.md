# Android ARM64 MX+ readiness correction — preview.35

This distinct private candidate is `0.1.1-preview.35` / `101035`. It follows
preview.34 without altering historical packages, sources or test evidence.
The physical preview.34 run had three pre-write Bluetooth connect failures and
three opens followed by `AT\r?\r\r>`. Those are separate failure phases.
The former AT/OK gate and success fixtures were not valid hardware qualification.

The shared workflow now treats a bounded completed `?`/STOPPED prompt as a live
channel, advances from AT to explicit ATI, and requires the validated ELM327
identification before either of two prompt-gated CR repeats. Both CRs repeat ATI;
no unknown previous vehicle command is repeated. A completed residual prompt
from the initial ATI permits one explicit ATI retry after a bounded quiet period.
A second rejection, malformed identity, noise, incomplete prompt, cancellation
or deadline ends the check before vehicle work. Sleep remains unverified; a lost
character fixture is a recovery case, not a diagnosis of an observed failure.

Rust and Java fixtures include the actual captured AT rejection and explicitly
model echoed ATI identification. Unlisted commands fail rather than returning OK.
The ATI reply fixture is an explicit expected wire format until independently
captured physical bytes have their own receipt; a mock is never labelled a
physical recording. Failure reports retain readiness evidence, separate from
Bluetooth connect/release facts. No automatic socket reconnect is added.

The existing production coordinator supports an adapter-only `identity` mode.
Its Android send boundary permits only AT, ATI, ATD, ATE0, STDI, STI and ATRV,
plus the shared typed repeat of confirmed ATI. It sends no VIN, CAN setup,
security request or firmware command, changes no working card, and saves no
ECM/DTC report. The owner-requested physical instrumentation uses the installed
production transport and exact installed native worker, checks the APK/worker
hashes and full ARM64 label, uses only the existing bond, and closes afterwards.
It is a separately versioned test helper, not a user interface or bundled feature.

The preview.34 retained-security-processing implementation remains. Neither this
readiness correction nor a successful adapter-only check fixes or explains a
failure inside BluetoothSocket.connect. HCI evidence, controlled no-button
close/reopen trials, independently observed sleep and the full retained-security
handoff still require physical results. Short 8–16-second reconnect gaps are not
proof of the documented 600-second UART sleep. No button fix is claimed.

Before a full garage run, check ATI, ATD, STDI/STI and clean release with zero
vehicle requests on the actual MX+. Software fixtures, installed identity and
this bounded physical check have separate receipts. Then all four exact-candidate
adapters still need the common golden path, including fault read, same-run
INIT_AUTH, preserved retry/import data, ECU acknowledgement/readback and final
cleanup. No beta/publication or hardware-protection qualification is implied.

The tracked retained-input builder archives the clean source, exact eight native
entries, pinned historical probe origins, signing identity and dependencies.
Six native entries are rebuilt; Nano/Chipsoft probes retain their explicit
preview.1 hashes. Root/MX role names intentionally share common-engine bytes.
The changed shared source is in this candidate's workflow crate. Canonical Apple,
iOS and other platform source/package adoption remains explicitly pending; no
old artifact inherits this change or its future checks. No operator password is
introduced; internal authentication, consent and SSA integrity checks remain.

Manufacturer reference: OBDLink FRPM F sections6,7.2,8.9,15.3 and15.7.
https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf
