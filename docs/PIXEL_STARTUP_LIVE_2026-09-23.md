# Pixel 7 installed startup validation — 23 September 2026

Signed ARM64 `0.1.0-preview.23` (version code 100023) was installed on the physical Pixel 7 over wireless ADB. No existing OpenSAAB package was present, so this was a fresh installation. APK SHA-256: `c905ee8e2ec5bbc05682fed62338c4a2f650ec7cd443104817fe916cbc0f6691`. Signature verification passed and matched the recorded OpenSAAB signer.

The installed UI completed its English / Saab NAO 9.250 download and returned to Connect. Android enumerated Chipsoft J2534 (0483:5740), and its USB permission prompt was accepted. Optional online vehicle lookup remained off.

## Observed result

- Connect and start opened the powered-ECM/ignition confirmation and startup engine-code workflow.
- A fresh valid VIN identified model year 2004; the VIN is retained only in private evidence.
- The installed startup sequence displayed a complete Trionic 8 current/history engine-code report over 500 kbit/s HS-CAN: P0107 current; P0122, P0223, P1682, P2122 and P2127 history.
- The app reported “Session ended · USB released,” “Complete report · codes not cleared,” and enabled Continue to diagnostic menus.
- Continue prompted for ignition confirmation again, then launched the connected native session and visibly reached the original Main Menu (Diagnostics, SPS, Tool Options, Diagnostic Strategy, Release Notes).

This closes the previously pending physical Pixel/ARM64 test of the integrated startup VIN + HS-CAN DTC sequence. It is installed-app evidence, not merely a standalone reader result. The repeated ignition confirmation is an observed extra step for future workflow review, not a failed handoff.

It does not qualify ARM32, other adapters, original-menu ECU/DTC retrieval, all-module coverage, full-vehicle security access, language switching in this run or physical unplug/replug recovery. No codes were cleared and no programming or security operation was selected. No precise startup benchmark was taken.

The app was left at the original Main Menu for the owner. Raw logs, screenshots, UI hierarchy, installed version evidence and the machine-readable receipt are held privately under the local OpenSAAB test archive; do not publish their VIN-bearing originals. Production releases were not changed by this test.
