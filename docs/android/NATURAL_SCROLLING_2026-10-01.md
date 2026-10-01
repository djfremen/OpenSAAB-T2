# Android Natural scrolling — implemented, not deployed

Owner request, 1 October 2026: add the iOS scrolling preference to Android,
retain it for a future release, and do not redeploy.

## User behavior

App menu → Preferences → Natural scrolling is available from the offline,
Chipsoft and Nano firmware screens, including during an active session.

- Default off: swipe up sends UP (9); swipe down sends DOWN (12).
- On: swipe up sends DOWN (12); swipe down sends UP (9).
- Android SharedPreferences saves `naturalScrolling` in `firmware-controls`.
  Each new stroke reads the choice; changing it needs no emulator restart.
- Only deliberate vertical strokes on the firmware display are affected.
  Keypad arrows, other buttons, hold-to-ENTER, taps and app list scrolling
  keep their behavior. Opening Preferences cancels any unfinished gesture.
- The same Java controls serve both Android ABIs. Separate app packages retain
  separate preferences; normal updates preserve them, uninstall/clear-data does not.

The common specification is `OpenSAAB/emulator/ui/interface-contract.json`,
SHA-256 `6ff3c66b8b285bcf3cc2513845e5c82946e57084529042f57ed89bf5f5b8d539`.
Android's native binding is checked against its preference key, label, default,
four direction mappings and two descriptions; no contract change was needed.

## Local validation

- `python3 scripts/android/test-firmware-scroll.py`: passed against the common
  contract. Use `--contract PATH` if the companion repository is elsewhere.
- All app Java sources and the extended `Tech2ControlsInstrumentedTest` compiled
  with SDK 36 and the existing pinned AndroidX dependencies (102 source files
  including generated resources). Log: `target/android-natural-scrolling-20261001/compile.log`.
- `python3 scripts/android/test-adapter-boundaries.py`: passed.
- `git diff --check`: passed.

The instrumented suite now covers both modes, unchanged keypad/hold/tap behavior,
immediate adoption on an existing display, saving/reopening Preferences and no
firmware keys from toggling. It restores the previous setting when finished.
**It was compiled, not executed.** No Android device was contacted; no test APK
or app APK was installed, and no release/version/catalog was changed.

## Release follow-up (still pending)

- Run the extended keypad suite on an isolated Android test installation after
  deployment is authorized; do not interrupt the owner's active emulator.
- Verify the App menu entry and readable switch/help text on offline, Chipsoft
  and Nano layouts, including compact and head-unit screens.
- Exercise actual original firmware menus in both modes on ARM64 and ARM32
  separately. Confirm one key per swipe, unchanged keyboard/keypad/hold/tap
  controls, no session restart and no command from changing the preference.
- Verify persistence across cold app relaunch and a normal upgrade, preserving
  firmware/card selection. Recreating a view alone does not prove these cases.
- Carry this local commit into the next approved Android candidate, run its
  checks, and update the shared matrix/dashboard with exact installed evidence.

Until then, published ARM64 preview.29 and existing ARM32 installations retain
their previous directional scrolling. Desktop adoption remains a separate gap.
