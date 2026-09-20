# OpenSAAB Setup — 2026-09-15

The website has one starting APK, **OpenSAAB-Setup.apk**. It upgrades the previous checker in place (same `com.opensaab.checker` package and release key), runs without native libraries on Android API21+, and checks API26+ and Android-supported ARM ABIs before offering an emulator.

Setup detects RAM/storage, warns about limited memory, and selects ARM64 when Android supports it, otherwise ARMv7. An existing ARM32-only installation stays on its channel even on dual-ABI Android. Compatible channels can be selected explicitly. Unsupported platforms get requirements instead of an unusable APK.

Setup downloads the selected emulator from official GitHub releases, verifies its SHA256, release signing certificate, package, version and native ABI, then opens Android's installer with a read-only grant. It never silently installs, uninstalls, downgrades, changes firmware or overwrites a differently signed development installation. Android requires the user's install-from-this-source permission and confirmation. Downloads stream to disk; incomplete files are discarded and can be retried. Verified downloads can be reused. Existing OpenSAAB can be opened offline.

**Firmware remains in the installed emulator app.** After installation, Open OpenSAAB leads to its existing software/language download and extraction flow; existing firmware is kept. Setup does not download a second copy or transfer files between package sandboxes. Internet is needed for downloads/security access; local USB diagnostics can work offline once configured.

## Independent release channels

`service/catalog/releases.json` is published at `https://www.opensaab.com/static/t2/releases.json`. It explicitly pins one version per architecture. Updating its ARM32 entry does not rebuild, republish or change the ARM64 artifact. The bootstrap and both emulator apps have independent versions. Catalog fetch is bounded and HTTPS only. APK redirects are restricted to GitHub's release hosts; the signer is pinned in Setup. No tokens or private keys are in the APK.

Initial catalog: ARM64 `v0.1.0-preview.2` / 100002; ARM32 `headunit-v0.1.0-headunit.3` / 100003. No emulator release changed as part of the installer rollout. Architecture indicator changes are committed in source but not in those existing emulator APKs.

Maintenance: `android/compatibility-checker/com/opensaab/checker/` (UI, catalog/download verification and narrow installer provider), `scripts/android/build-compatibility-checker.py` (signed build), `android/tests/SetupInstrumentedTest.java` (disposable emulator tests), `service/catalog/releases.json` (published releases). Website: `$PROJECTS/opensaab-research-update/static/t2/`.

Release process: publish/test an emulator APK first; verify its signature/hash/source receipt; update only its catalog entry. Keep archived APKs/source available on GitHub for reproducibility and testing, while the website offers Setup as the single download. No automatic architecture migration or silent updates.

## Adaptive Setup layout (0.3.1)

Setup measures usable window width/height after system insets and converts pixels to density-independent units. Landscape windows at least 720 dp wide use two independently scrollable columns: introduction and installation controls. Narrow/portrait windows use one scrollable column capped at 600 dp. The landscape threshold grows with larger font settings. This is not tied to a device model, CPU architecture, fixed resolution or assumed phone aspect ratio. Rotation/window resizing reuses controls and preserves selected release, download progress and verified APK state. Side/top/bottom system insets are respected.

## Existing development installation (0.3.2)

An APK signed by a different key cannot update the installed package. Setup detects this before downloading, disables the public update action, keeps Open existing OpenSAAB available, and explains backup/migration. It never recommends retrying a signing conflict or calls it the same update channel. Missing versionName is shown as Version not reported with its build number. No automatic uninstall, private-data copying, or signature bypass. Archive signature checks remain in place; installed-package checks also run again before installation.

## Optional Setup cleanup (0.3.3)

After confirming a release-signed, launchable emulator at the selected version or newer, Setup offers Remove Setup / Keep Setup once per emulator package. No prompt is shown for absent apps, signing conflicts, incomplete updates or active downloads. Keeping/cancelling does not repeatedly nag; a Remove Setup button remains available, including offline for an already verified installed app. Android confirms self-uninstallation, targeting only `com.opensaab.checker`. The emulator and its private firmware/reports/settings are separate and untouched. The main app has its own Check for updates. Opening it closes Setup’s task in Recents. The browser’s original downloaded Setup APK may still remain in Downloads; Setup does not delete another app’s files.

## Continuous onboarding (0.3.4)

The same flow serves ARM64 and ARM32. Install OpenSAAB downloads and verifies the selected release, resumes once after returning from Android source permission, and opens only the recognized signed package at the selected version or newer. A persisted release descriptor binds continuation to that package, hash and version. Permission refusal and installation cancellation leave a retry state; they do not loop dialogs. Returning from a process restart rechecks permission and installed identity. Required Android confirmations remain. Setup removes its own verified APK after successful handoff; browser downloads remain under browser/user control.

Cleanup never interrupts first use. Once diagnostic firmware is ready, the main app offers Remove installer in its app menu, with a pinned Setup signer and a fixed package-only uninstall intent. Android confirms removal. The Setup app still has a manual cleanup action.

First-run firmware preparation starts with software/language selection, then Download and continue. A successful, verified first-run download or import returns to the main connection screen automatically. Advanced version changes retain the normal completion view and backups. Errors and cancellation do not hand off.

The normal launcher always offers adapter connection, including optimized ARM32. A single supported adapter skips the picker; multiple candidates keep selection; offline emulation remains an explicit app-menu option. ARM32's accelerated native boot and deferred CANdi configuration remain architecture-specific. No native ARM64 behavior changes.

Chipsoft's key-ON confirmation authorizes a fresh VIN read followed by firmware startup. Optional online engine/color lookup is unchecked by default, discloses the VIN destination, and runs after the local VIN result without holding the firmware startup path. Session generations reject stale lookup results. ECU information uses exact recognized original Engine/Engine Control menu labels for a known 9440 vehicle; unavailable identity or unfamiliar screens leave manual navigation available. The shortcut does not bypass vehicle-network checks.

The observed Chipsoft 001D rejection is explained as a single-wire wake failure only when the most recent CAN TX intent in the session trace matches that wake request. VIN success does not imply ECU-information/DTC success, and the status is not represented as a confirmed CAN transmission.

## Development follow-up — September 19

Home now exposes **Run without an adapter** directly, with a dismissible first-frame controls guide shared by offline and adapter sessions. Missing firmware returns to setup. These changes are development-source behavior; the earlier preview.22/headunit.17 release description above remains historical. See [Studio verification and the direct-APK assessment](STUDIO_ONBOARDING_2026-09-19.md).
