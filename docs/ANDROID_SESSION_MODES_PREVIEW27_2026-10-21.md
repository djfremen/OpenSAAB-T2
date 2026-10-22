# Session-mode clarity and recovery, preview.27

Base: 0.1.1-preview.26 (`bc5db025`). Android Java only. The preview.26 native components are reused unchanged, so record their existing hashes with the new APK.

## Why

The Oct 21 garage tests on the Fire HD 10 used four adapters. They showed these failures:

- Chipsoft ran Read-only (the default on every launch). Its guard blocked `$27 01`, and the session ended with no explanation.
- The Nano Full session was refused on release builds, and the only sign was a log line.
- After a security import, MX+ *Continue firmware* only changed a status line.
- Nano *Continue firmware* in a fresh screen always said "Reconnect Nano".
- The security reset was buried two menus deep as "Clear offset".

## Changes

| Area | Change | Files |
| --- | --- | --- |
| Nano Full | Available on every build. Exclusive-profile and adapter-test conflicts still reject it. | `NativeCommandGate.java` |
| Mode visibility | A new `SessionModeBadge` appears on the Ready chooser, on the firmware workspace (outside the collapsible header) and on the Chipsoft, Nano and MDI screens. FULL is a warning-colored badge that says vehicle changes are possible. READ-ONLY says changes are blocked. *Connect and start* names the mode. | `SessionModeBadge.java`, `SessionModes.java`, `SessionWorkspace.java`, the activities |
| Mode memory | The Ready choice is saved. On first run the default is still Read-only. *Restart in Full* saves Full. | `SessionModeControl.java`, `MainActivity.java` |
| Read-only refusal | The existing guard log wording is recognized. The app explains which request was blocked and that it was not sent, then offers **Restart in Full session**. No restart happens without the tap. | `PolicyBlock.java`, `FullSessionPrompt.java`, `ChipsoftUsbActivity.java`, `NanoProbeActivity.java` |
| MX+ resume | *Continue firmware* reconnects the same adapter in Full, or in the Clear/Reset mode that was running, and re-checks the VIN. Moving from Read-only or collection to Full asks first, with the mode named. There is no automatic Bluetooth reconnect after an import. | `MainActivity.java`, `SecurityAccessView.java` |
| Nano resume | With no confirmed session in the current screen, *Continue firmware* now asks "Connect again in Full session?". The screen's mode is never switched automatically, and an unconfirmed cleanup still blocks the in-place resume. | `NanoProbeActivity.java` |
| Security reset | **Reset to INIT_AUTH** appears on every security panel. It uses the existing NoMoreGlobal reset, with backup and receipt. | `SecurityAccessView.java`, `SecurityReset.java`, `AdvancedFeatures.java` |
| Reporting | Bluetooth connect failures keep the watchdog's real reason (timeout before cancel). The Chipsoft Full prompt appears only after a confirmed USB release, without a second "connection problem" report. The MDI collection badge says MDI has no read-only guard. An MDI USB detach is recorded as `DISCONNECTED`, and its crash summary reads `native/`. | `VlinkerSppTransport.java`, `MdiUsbActivity.java` |

## Review

A separate review of the diff found 10 issues. Nine are fixed here. They covered the timeout reason, the Nano auto-resume after an unconfirmed cleanup, test preference contamination, the MDI collection wording, the MX+ Read → Full confirmation, the Chipsoft gating, badge relayout on every poll, the MDI connect label and Nano badge coverage. The picker instrumented test now clears the remembered mode first. The tenth, the badge staying visible on idle Chipsoft and MDI screens, is kept on purpose: those screens always start in their shown mode.

## Checks done

- `javac` against android-36. The patched tree produces exactly the same error set as preview.26: 80 errors, all from androidx-only report classes that need the Gradle dependency jars.
- `scripts/android/test-request-gate.py`: all suites pass. This includes the updated Nano Full gate tests and the new `PolicyBlockTest`, which uses the exact Fire Chipsoft line from Oct 21.
- `test-build-profiles.py` fails identically on preview.26. It needs the native ARM64 pins.

## Not done

- Gradle/APK build, install, instrumented tests and the vehicle run.
- The instrumented `SessionModePickerInstrumentedTest` assumes a fresh install starts Read-only. That is still true, but a reused emulator now remembers the last choice.
- Changed `reference_files` hashes in the interface contract are not refreshed yet.
- Not covered: the UI-thread console batching, the MDI native teardown order and the crash-snapshot ordering from the Oct 21 audit.
