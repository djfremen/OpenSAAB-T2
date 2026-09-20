# Android diagnostic languages and VIN/DTC startup

Shared implementation for ARM64 phones and experimental ARM32 head units. This branch starts from the current Android onboarding/adapter organization and incorporates the previously bench-tested direct Chipsoft HS-CAN engine-code reader.

## Language installation

Setup now separates **Diagnostic language** from **Software version**. English retains the NAO 9.250 default and Tech2Win 148.000. German, Spanish, Finnish, French, Italian, Dutch, Russian and Swedish use separate original Saab 148.000 program images. The older Russian 140.500 image remains download-only because it does not boot in this emulator profile.

The external Tech2Wiki ZIP and extracted image are SHA-256 checked. The existing transactional firmware store keeps immutable originals, backs up a modified working card, preserves it on cancellation, and journals activation. Active metadata records the diagnostic language; reopening selection follows the active image. No language binaries are committed or embedded in the APK.

This changes the original diagnostic menus, not the OpenSAAB Android controls, which remain English. Automatic menu shortcuts currently require a recognized English card. Other languages use the original firmware controls; English label matching is not attempted against untranslated or unrecognized menus. Vehicle communication and security workflows have not been validated in every language.

## Chipsoft startup

The normal home **Connect and start** action, when Chipsoft is selected, now opens a bounded startup check:

1. Confirm a powered Trionic 8 bench ECM or ignition ON; online vehicle details remain optional.
2. Read a fresh VIN and release the VIN probe's USB ownership.
3. Revalidate the selected device and permission, then read current/history Trionic 8 engine codes over 500 kbit/s HS-CAN.
4. Save a complete, timestamped VIN-bound report, release USB, and enable **Continue to diagnostic menus**. The subsequent native session identifies the vehicle again.

This read neither clears codes nor requires the low-speed bus. It is a Trionic 8 ECM check, not an all-module scan. Unsupported/absent adapters and incomplete reports must not become a zero-code success. Stop/cancel invalidates continuation. The direct engine-code action is also available separately; existing explicit firmware shortcuts remain accessible.

Chipsoft commands and transport remain under `src/adapters/chipsoft` and `android/adapters/chipsoft`. The common byte transport is shared through `adapters::common::usb`; no Nano wire implementation is imported. Nano keeps its existing startup path and is not claimed to have this direct DTC feature.

## Validation on September 20

- Rust: 113 passed, 7 intentionally ignored, zero failed.
- Java host checks: catalog hashes/selection, adapter routing and boundaries, request ordering, command gates, DTC handling, navigation and existing shared checks passed.
- Android Studio ARM64 test device: real language spinner callbacks/persisted choice, startup cancellation, immutable firmware originals/backup/cancellation/recovery, actual HTTPS download/retry/version switching, onboarding completion and existing ignition/optional-lookup tests passed.
- Physical Pixel 7 over Wi-Fi ADB: all ten runnable program images reached the original main menu in offline native runs. Russian LCD output was visually checked for Cyrillic rendering.
- ARM64 and ARMv7 native builds compiled. ARM32 was not installed or retested on a physical head unit during this change.

Observed Pixel menu times from starting each native process, including scripted ENTER and ADB observation (not a controlled benchmark):

| Image | Seconds |
| --- | ---: |
| English NAO 9.250 | 5.75 |
| English 148.000 | 5.89 |
| German 148.000 | 3.54 |
| Spanish 148.000 | 6.06 |
| Finnish 148.000 | 5.87 |
| French 148.000 | 6.38 |
| Italian 148.000 | 5.95 |
| Dutch 148.000 | 5.79 |
| Russian 148.000 | 5.86 |
| Swedish 148.000 | 8.22 |

Pixel currently has no USB adapter attached. The earlier Chipsoft/Trionic 8 bench result supports the underlying DTC reader, but does not substitute for a live test of this new startup integration on Pixel. Pixel was unlocked and the real language-selector/startup-cancellation and transactional firmware-store tests passed. The old app and downloaded setup APK were removed. The requested clean-install MP4 is pending Wi-Fi reconnection: both ADB transports dropped before the recorder started or the new APK downloaded. Nothing has been published to the production download site by this task.

Private local evidence: `~/.local/share/opensaab/pixel-languages-20260920/`. This includes the previous Pixel APK, language LCD frames and native logs; it is not committed.

## Reproduction

Run `cargo test --locked --no-default-features --lib` and `python3 scripts/android/test-request-gate.py`. Build native executables with `scripts/android/build-headless.sh arm64` or `headunit-arm32`, then the matching `scripts/android/build-tech2-app.py` profile.

With a stopped, debug-signed app on an unlocked test device, run `scripts/android/test-lcd-on-device.py --serial SERIAL --suite languages` and `--suite firmware`. The `firmware-download` and `startup` suites require the disposable Android emulator. The download suite requires a fresh app data directory.

For language boot checks, use the same packaged ARM64 executable and approved eprom/opsys/CANdi support files, a working copy of the catalog-pinned card, `--interactive-headless --research-harness --candi-native-link`, and a unique output directory. Send `enter` through `interactive-key.txt` at the splash and confirm both main-menu F0/F1 entries plus the LCD image. Send `stop` and preserve the output without committing firmware binaries.
