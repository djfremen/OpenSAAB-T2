# Android MX+ manual-session startup

OpenSAAB 0.1.1-preview.9 ARM64 corrects the runtime validation of the existing
MX+ manual session. Preview.8 paired successfully and its separate connection
probe read the VIN, but MX+ firmware startup exited 2 with
`--harness-target requires --test-harness`, before any LCD frame or input.

The MX+ token, interactive manual mode, native link, research flag, exact target
and exclusive adapter checks already run before the general target check.
That final check now accepts the previously validated MX+ manual session.
Automatic harness navigation, replay, another bridge, a malformed token and a
non-manual target remain rejected. A regression test reproduces the Android
argument list and those rejection cases.

No CAN request policy changed. The MX+ original-firmware path remains reads
plus its captured key-status operation. Counter reset is unavailable here;
Nano, Chipsoft and MDI results cannot qualify MX+. The primary Connect and
start discovery path and the separate App menu / MX+ firmware session remain
distinct in this preview. This is an unresolved workflow-parity gap.

The six legacy/adapter/connection workers retain the exact preview.1 hashes.
Only the recovered runtime changes. Preview.8 APK and source evidence remain
immutable. Preview.9 is unpublished and its physical outcomes must be recorded
against the installed APK hash separately. No uniform qualification is claimed.
