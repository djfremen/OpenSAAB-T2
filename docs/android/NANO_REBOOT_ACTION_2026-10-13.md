# Explicit Reboot Nano action

Owner request: selecting Nano should make a Reboot Nano function available.
This extends the manual/guided security candidate; it does not execute a reboot
on the owner's phone or change the pending stopped collection screen.

Ready → Actions shows Reboot Nano when a Nano USB candidate is attached. With
multiple Nanos, choose the target explicitly. A Nano firmware session exposes
Reboot Nano in Actions. During an active session, Stop and reboot ends that
session, waits at most15 seconds for confirmed native/USB cleanup, verifies the
same selected USB identity and starts a separate adapter-only request. Leaving
the screen, unplugging, failed cleanup, timeout or changed selection cancels the
handoff. Processing must finish before reboot. There is no automatic reconnect,
VIN/seed request or diagnostic replay after reboot.

The existing Rust probe performs echo, qualified VCX-NANO /1.9.4.2 identity,
software reboot opcode0x8d and exact zero-status acknowledgement, then closes
USB. NanoStartupGate retains the bounded envelope/identity/ack sequence. The
result requires the native acknowledgement and Java gate acknowledgement,
zero vehicle transmissions and diagnostic writes, clean channel/USB closure and
no cleanup errors. USB readiness alone is insufficient. Success says Nano
software reboot acknowledged; this does not claim a physical power cycle or
re-enumeration. The next connection requires Connect and start.

Reboot is available without a diagnostic firmware image or debug privileges;
initialization/skip-init/private diagnostic modes retain their existing guards.
Reboot and diagnostic/seed/channel/initialization modes cannot be combined.
The USB target is checked again after permission. Attachment never starts reboot.
Collected security evidence/card/receipts remain untouched.

New tests cover single consumption, active ownership, cleanup failure, deadline,
background/unplug cancellation and strict acknowledgement/zero-vehicle receipt.
Nondebug Android instrumentation exercises the real public-mode entry with no
Nano attached: visible action, missing-adapter failure, retry and mixed-mode
rejection, without firmware/USB/API traffic. The three existing security suites
are included. Actual hardware reboot plus a subsequent fresh connection remains
an owner-operated test on the exact candidate.

Private candidate:0.1.0-preview.67-security.3 ARM64 /versionCode100068, retaining
all preview66 native/support payload bytes. Previous candidates remain archived.
Shared Rust protocol already exists; common desktop menu/lifecycle adoption and
all exact installed physical qualification remain pending. Linux is excluded
from the October13 vehicle campaign. No public publication/updater change.
