# OpenSAAB Setup — 2026-09-15

The website has one starting APK, **OpenSAAB-Setup.apk**. It upgrades the previous checker in place (same `com.opensaab.checker` package and release key), runs without native libraries on Android API21+, and checks API26+ and Android-supported ARM ABIs before offering an emulator.

Setup detects RAM/storage, warns about limited memory, and selects ARM64 when Android supports it, otherwise ARMv7. An existing ARM32-only installation stays on its channel even on dual-ABI Android. Compatible channels can be selected explicitly. Unsupported platforms get requirements instead of an unusable APK.

Setup downloads the selected emulator from official GitHub releases, verifies its SHA256, release signing certificate, package, version and native ABI, then opens Android's installer with a read-only grant. It never silently installs, uninstalls, downgrades, changes firmware or overwrites a differently signed development installation. Android requires the user's install-from-this-source permission and confirmation. Downloads stream to disk; incomplete files are discarded and can be retried. Verified downloads can be reused. Existing OpenSAAB can be opened offline.

**Firmware remains in the installed emulator app.** After installation, Open OpenSAAB leads to its existing software/language download and extraction flow; existing firmware is kept. Setup does not download a second copy or transfer files between package sandboxes. Internet is needed for downloads/security access; local USB diagnostics can work offline once configured.

## Independent release channels

`service/catalog/releases.json` is published at `https://www.opensaab.com/static/t2/releases.json`. It explicitly pins one version per architecture. Updating its ARM32 entry does not rebuild, republish or change the ARM64 artifact. The bootstrap and both emulator apps have independent versions. Catalog fetch is bounded and HTTPS only. APK redirects are restricted to GitHub's release hosts; the signer is pinned in Setup. No tokens or private keys are in the APK.

Initial catalog: ARM64 `v0.1.0-preview.2` / 100002; ARM32 `headunit-v0.1.0-headunit.3` / 100003. No emulator release changed as part of the installer rollout. Architecture indicator changes are committed in source but not in those existing emulator APKs.

Maintenance: `android/compatibility-checker/com/opensaab/checker/` (UI, catalog/download verification and narrow installer provider), `scripts/android/build-compatibility-checker.py` (signed build), `android/tests/SetupInstrumentedTest.java` (disposable emulator tests), `service/catalog/releases.json` (published releases). Website: `/Users/mini4/Documents/Projects/opensaab-research-update/static/t2/`.

Release process: publish/test an emulator APK first; verify its signature/hash/source receipt; update only its catalog entry. Keep archived APKs/source available on GitHub for reproducibility and testing, while the website offers Setup as the single download. No automatic architecture migration or silent updates.

## Adaptive Setup layout (0.3.1)

Setup measures usable window width/height after system insets and converts pixels to density-independent units. Landscape windows at least 720 dp wide use two independently scrollable columns: introduction and installation controls. Narrow/portrait windows use one scrollable column capped at 600 dp. The landscape threshold grows with larger font settings. This is not tied to a device model, CPU architecture, fixed resolution or assumed phone aspect ratio. Rotation/window resizing reuses controls and preserves selected release, download progress and verified APK state. Side/top/bottom system insets are respected.
