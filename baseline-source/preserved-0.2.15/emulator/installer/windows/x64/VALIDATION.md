# EliteBook MSI validation — 2026-09-19

Package: `OpenSAAB-0.1.1-x64.msi`, 3,244,032 bytes. SHA-256: `DAD3800A426088B4712C1383B36E3007122BC95D0B141D0A7E0AF1A213BD960B`.

Built using WiX 5.0.2 with validation enabled on Windows 10 Pro x64 build 19045. This is an unsigned, private offline test package. The copied runtime images are private build inputs and are not in Git.

The existing development install and runtime images were backed up and verified by hash before removing the active legacy folder and its shortcut. Unrelated software and manufacturer drivers were left installed.

Verified through Windows Installer, with verbose logs:

- Install returned 0. All ten payload hashes, three shortcuts and version 0.1.1 registration matched.
- Removing the installed launcher then running MSI repair restored it; all payload hashes matched again.
- Uninstall returned 0. Packaged files, shortcuts and installed-program registration were removed. A test file in the separate user-data folder survived.
- Reinstall returned 0 and repeated all file/shortcut/registration checks successfully.
- The reinstalled launcher started the emulator from `WindowsApp`, with logs in `WindowsData`. Verified guest welcome appeared at 835 ms; CANdi had not started.
- The running process loaded `VCRUNTIME140.dll` from its installed app folder, confirming the app-local dependency is used.

The first registration assertion incorrectly inspected HKCU only. Windows Installer registered the current-user product with user-SID data and an HKLM uninstall entry. The test was corrected to inspect both uninstall locations, then the full cycle passed. Original failure evidence is retained alongside the successful cycle.

After the user unlocked Windows, verified welcome → Main Menu → Diagnostics/model-year selection, Down (2012 → 2011), and Escape back to Main Menu. The remote harness initially omitted keyboard scan codes; corrected its SendInput mapping and extended navigation-key flag, retaining all desktop/focus guards. Seven harness tests passed on Windows, including scan-code and Unicode regression checks. No emulator binary changes were needed.

Visible maintenance repair of the previously elevated SSH installation failed with errors 1310/1304 and then 2349 while replacing LICENSE/OpenSAAB.exe. The launcher and emulator were no longer running. After uninstalling that installation, a fresh installation through the normal interactive desktop completed, followed by a successful visible Repair wizard. All ten installed payload hashes matched afterward. This isolates a mixed-elevation installation/repair scenario but does not establish its exact cause or fix that scenario generally. Failure screenshots and observed log excerpts are recorded; the initial full repair log was overwritten when the task was reused. The subsequent fresh-install and repair logs have separate retained names.

No new ECU test or clean-machine/Windows 11 qualification is claimed. A future-version upgrade was not tested. The final state is the new MSI installed through the normal desktop and the emulator running in emulation mode. Future validation must include normal-user installation/repair as well as the elevated command-line lifecycle.

Private evidence lives on the EliteBook under `%USERPROFILE%\OpenSAAB\InstallerTest-20260919`, with a copy on the development Mac under `~/.local/share/opensaab/windows-installer-test-20260919`. The legacy backup is under the EliteBook evidence root's `backup` directory.
