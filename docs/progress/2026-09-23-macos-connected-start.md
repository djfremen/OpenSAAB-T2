# OpenSAAB progress — September 23, 2026

Our goal is the same setup, original diagnostic menus and supported adapter
behavior on Android, macOS, Windows and Linux. Today's checkpoint closes an
important Mac launch gap; it does not qualify all platforms as equivalent.

## Installed M4 build: macOS 0.2.7 development preview

Connect and start now reads a fresh Chipsoft Pro VIN from the bench ECM and
launches the original Saab 9.250 firmware. Previously that action stopped after
the VIN helper. The native adapter bridge now starts when the firmware needs it,
reusing the existing adapter protocol and read-command policy.

The installed build reached Engine Control menus and received 541 real CAN
frames through the bridge. The original single-wire wake was rejected on our
ECM-only bench, which lacks diagnostic pin 1/SW-CAN. The app now explains that
specific failure. Adapter shutdown released the port, and retry read a fresh
VIN and relaunched the original firmware successfully.

**Original ECU-information and DTC retrieval are not confirmed by this test.**
Further validation with the required vehicle buses is still needed.

Mac setup work includes original support components and a guided diagnostic
software download. ARM64 and Intel 0.2.7 packages were built from the same clean
source revision and archived privately with checksums. The final ARM64 build
passed 16 setup/self-tests and 19 offline runtime checks. Intel passed 16
self-tests under Rosetta; its native 0.2.7 hardware retest remains pending.
Earlier Intel setup/VIN evidence is not inherited as a pass for the new build.

There is no new public desktop installer in this update. Desktop packages are
private development previews, ad-hoc signed and not notarized. Remaining work
includes vehicle validation, startup DTC integration, physical USB reconnect,
full interface conformance, and installation videos.

## Help with adapter captures

We are adapting OpenSAAB-Collector into a separate capture-only track for
contributors using their already-working Tech2Win installation and adapter.
Start with initialization and a VIN read; then ECM information and DTC reads.
No code clearing, programming or security-access request is needed for the first
sample. Keep raw captures private and agree on a transfer destination first.

[Windows / Mongoose USB capture guide](../contributing/USB_CAPTURE_GUIDE.md)

A capture provides evidence for implementing and testing adapter support; one
file is not a guarantee that every required command is known.

[Project website](https://www.opensaab.com) ·
[Support development](https://ko-fi.com/djfremen)
