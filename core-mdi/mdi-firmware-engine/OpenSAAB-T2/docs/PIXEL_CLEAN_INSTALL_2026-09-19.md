# Pixel 7 clean installation and offline workflow

Recorded September 19, 2026 through wireless ADB on the M4 Mac mini. Continuous video: 223.34 seconds. 29 recorded input events, including optional cleanup and one redundant menu tap. Remote-control pauses are retained; this is not an installation-speed benchmark.

## Result

Removed the existing `com.opensaab.tech2` app and its app data. No other OpenSAAB package was installed. Removed old Setup APKs 0.3.1, 0.3.2 and 0.3.3 from Downloads. No unrelated apps or downloads were removed.

Installed Setup 0.3.4 directly from the public website, then ARM64 0.1.0-preview.22 (100022). Both apps passed the prompted Play Protect scan. Chrome already had installation-source permission; Setup's permission was newly granted and installation resumed automatically. The main app opened without an extra Open action.

Downloaded Saab NAO 9.250 English from the app and automatically returned to Connect. Used App menu → Run without an adapter, entered the original main menu, opened Diagnostics/model years, viewed the keypad, moved selection and returned with EXIT. No adapter was connected, no VIN probe ran, and no real ECU commands were sent.

Stopped emulation and removed Setup through the main app's optional Remove installer action. Android returned to the main app. Preview.22 and its Saab software remain installed. The current browser-downloaded Setup APK is retained; old APKs were removed before recording.

## Video chapters

| Start | Stage | What is documented |
| --- | --- | --- |
| 00:00 | Remove the previous app | Confirm Android uninstall. Old Setup APKs were removed before recording. Unrelated apps and downloads were retained. |
| 00:15 | Download from the website | The homepage downloads Setup 0.3.4 directly. Chrome already had permission to install apps on this phone. |
| 00:30 | Install and scan Setup | Android installation and Play Protect scan are retained. Play Protect reported that the app looks safe. |
| 01:07 | Install the matching app | Setup automatically selects ARM64 preview.22. Tap Install OpenSAAB; the download is verified. |
| 01:23 | Allow Setup installation | Enable Allow from this source for Setup and return. Installation resumes automatically. Scan and confirm the main app. |
| 02:06 | Prepare Saab software | OpenSAAB opens automatically. Choose Saab NAO 9.250 English and tap Download and continue. Setup verifies and extracts it, then returns to Connect. |
| 02:20 | Start without an adapter | App menu → Run without an adapter. No USB connection, VIN probe or vehicle session is started. |
| 02:35 | Explore original menus | Hold the display for ENTER. Open Diagnostics to view model years, inspect the keypad, swipe to move selection, and use EXIT to return. No ECU information or codes are read. |
| 03:10 | Optional cleanup after first use | Stop emulation, then App menu → Remove installer. Confirm removal of Setup. The main app and downloaded Saab software remain. |

## Scope

This recording validates the published installer and offline ARM64 workflow. It does not validate Chipsoft, VCX Nano, a VIN read, live diagnostics, security access, DTC reading or ECM software identification. The repository adapter reorganization is separate work and is not contained in the published APK shown here.
