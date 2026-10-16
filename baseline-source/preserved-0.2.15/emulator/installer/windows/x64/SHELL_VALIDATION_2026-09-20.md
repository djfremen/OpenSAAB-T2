# Android-reference Windows shell — 2026-09-20

## Exact final candidate

- Private unsigned package: `OpenSAAB-0.2.3-x64.msi`, 3284992 bytes.
- SHA-256: `8e7712013eb371155060b0b622790ea9000812c72cf80f645f35602408f04a23`.
- Native application architecture: x64, not ARM64. Windows 11 ARM uses compatibility support.
- Sources: Windows shell/build/tests in this directory; Chipsoft identification reader in `emulator/scripts/windows/chipsoft`; pinned Android firmware metadata in `emulator/ui`.
- Emulator executable reused from the existing private 0.1.1 Windows payload. This work changes the frontend/installer and packages the existing native bridges; it does not reconcile all Android/desktop Rust differences.
- Private package/evidence folder on the M4: `~/.local/share/opensaab/windows-shell-20260920/`. Raw VIN/adapter logs remain there, outside Git.

## Verified

1. Windows build: .NET Framework x64 shell and separate Chipsoft x86 bridge compile; WiX 5 MSI validation passes. Final shell self-tests pass from the private payload on the Windows 10 EliteBook without opening hardware: catalog, corrupt-image rejection, blocked firmware, RGB framebuffer, truncated frame, partial DTC, complete empty DTC, cleanup failure.
2. Real Windows emulator runtime, using the new session/firmware classes: verified English and German downloads; actual framebuffer; original English Main Menu and Model Year; EXIT returns to Main Menu; German Hauptmenü/Fahrzeugdiagnose; graceful session termination; original firmware checksums unchanged; cancelled download preserves the active selection. `test-runtime.ps1` reproduces these tests without an adapter. This is runtime integration evidence, not a mouse/touch UI test.
3. EliteBook Chipsoft native helper tests: fresh VIN from service 1A 90, real ISO-TP ECU reply; six T8 engine DTCs from HS-CAN, explicit end of report, positive diagnostic-session termination, both filters stopped, channel disconnected and device closed. Codes: P0107, P0122, P0223, P1682, P2122, P2127. No codes cleared. These tests exercised the packaged helper scripts; the full installed Connect-button sequence still needs observation.
4. M4 Windows 11 ARM VM: preliminary 0.2.0 package installed; the new native Windows shell and firmware-selection dialog visibly launched. Downloading the default English image visibly reached “Verified and selected. Ready to start.” Its package SHA was `0bb6393591792d91c93ba372d751de741841770ea6bb974bf25950785021ede2`. After unlocking, the actual English Tech2 9.250 splash was observed. Closing exposed a timer/async-stop disposal race; fixed by detaching the session before awaiting shutdown (commit `2d70038`). Rebuilt 0.2.1 passes the eight payload checks on the EliteBook. The VM subsequently upgraded to 0.2.2 successfully; the final 0.2.3 redraw correction is built but not yet installed because the Mac locked again.

## Corrections after the preliminary VM install

Removed a parent-held adapter lock that would have prevented the native bridge from acquiring its own lock. Kept the shared watchdog/bridge lock, blocked overlapping UI operations, fixed Windows framebuffer file sharing for atomic replacement, bounded console logs, added download cancellation/deadline and corrected compact label spacing. Versions 0.2.1, 0.2.2 and 0.2.3 use increasing MSI versions for upgrades. Version 0.2.2 gives long connection status messages enough vertical space. Version 0.2.3 enables ResizeRedraw on the framebuffer control after the VM test exposed stale image fragments when maximizing a static screen.

## VM follow-up

The preliminary preview displayed an unhandled null-reference error while closing. The UI timer had disposed and cleared the shared session during the asynchronous stop. The corrected build retains exclusive ownership of the stopping session until termination and disposal finish.

### Exact observed VM evidence

- Windows 11 ARM, M4 VMware Fusion. Installed x64 preview 0.2.2 SHA-256 `77a8bb2bc871a481fa6e2a9f446d5f0016dfdfa9b20c907202260d8d1fc79e93` (3284992 bytes).
- MSI upgrade exit 0; all 15 installed payload hashes matched; active English firmware was retained; eight shell self-tests passed inside the VM. Evidence receipt: `evidence-1789954949764260000.json` in the private evidence folder.
- Corrected 0.2.1 UI: real English splash/Main Menu/Model Year; keyboard ENTER; Down moved 2012 to 2011; on-screen EXIT returned to Main Menu; keypad F2 opened Tool Options; keyboard F4 opened original contrast instructions. Closing during active emulation succeeded without the previous exception.
- Final installed 0.2.2 UI: full no-adapter guidance is readable; German download reached verified/selected; offline boot showed version 148.000 and ENTER reached Hauptmenü/Fahrzeugdiagnose. Closing this session was requested before the Mac locked; its completion was not observed.
- Maximizing 0.2.2 left stale framebuffer fragments. This is an observed failure, not a passed resize test. Source correction is in 0.2.3; build and eight payload guards pass on the EliteBook, but the exact VM retest remains pending.
- Private screenshots: `vm-install-0.2.2.png`, `vm-no-adapter-0.2.2.png`, `vm-german-menu-0.2.2.png`. No VIN is included in these screenshots.
- A temporary PowerShell verifier stalled and consumed about 2.6 GB after fetching its JSON manifest. It was stopped; the precise cause is unresolved. A smaller explicit file-hash verification completed normally. The VM recovered after terminating that test process. This was separate from the emulator shutdown race.

## Remaining gates

- Unlock the M4 and upgrade to exact 0.2.3. Retest maximize/restore and display expansion, switch German back to English, and finish final-build stop/reopen. S1–S4 are rendered but an end-to-end firmware soft-key action is not yet qualified. Do not transfer observed 0.2.1/0.2.2 checks automatically to the final artifact.
- Observe installed Connect → fresh VIN → complete engine DTC → original firmware on the EliteBook; test disconnect/reconnect/cancel. No Chipsoft is connected to the M4 VM, and x64 application success does not establish ARM64 kernel-driver support.
- Complete Android parity for security access, automatic engine-data/DTC navigation, clearing DTCs, report upload, app updates and all desired firmware import/recovery controls. Current Windows controls remain English, matching the reference fallback rather than implying translated application text.
- Retest 0.2.3 install/repair/uninstall/upgrade with the updated lifecycle test, then qualify a clean supported Windows system. This private package still includes firmware and is not a public distribution candidate.
- macOS/Linux retain their existing frontend. The new interface-workflow release gate prevents qualifying them from shared engine code alone.

No public release or GitHub push is claimed by this report.
