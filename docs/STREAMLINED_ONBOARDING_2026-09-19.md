# Shared Android onboarding: September 19, 2026

The shorter Install → Prepare software → Connect flow is implemented for both ARM64 and ARM32. Signed builds are installed on the test devices and published as developer previews. **Public rollout completed September 19, 2026 (Pacific time): all three releases and the website catalog are live and verified.**

## User-visible workflow

| Stage | Previous recorded friction | New behavior |
| --- | --- | --- |
| Website | Separate download guide before the APK | Staged homepage offers Setup directly; guide remains secondary |
| Install | Download, Install, source permission, return, Open | One Install OpenSAAB action; resume after permission and open the verified installed app automatically |
| Prepare | Welcome, software choice, completion gate | Software choice with Download and continue; verified preparation returns to connection automatically |
| Connect | Adapter picker, VIN, second Start prompt | One detected supported adapter skips picker; key-ON confirmation, fresh VIN, then startup |
| Vehicle details | Optional enrichment delays handoff | Explicit unchecked online option; lookup runs in background when selected |
| Cleanup | Uninstall interrupts first use | Optional Remove installer in the ready app; Android still confirms |
| Original menus | Repeated manual navigation | ECU information shortcut for known supported model/year; original network checks and manual menus remain |

Android installation/USB confirmations and signature, hash, ABI and package checks remain. Permission or install cancellation leaves a retry action. Setup persists the verified pending release across process restart. Successful installation deletes its own temporary APK, not browser-owned downloads. ARM32 native acceleration remains architecture-specific; the shared flow does not alter ARM64 native startup.

CANdi remains deferred until requested by original firmware. A fresh local VIN starts the firmware without waiting for optional online details. The ECU-information shortcut needs known vehicle-model information; without it, the user can use original menus. No network response is fabricated to bypass a missing bus.

## Builds

All APKs were built from `b8a24a485bda20770fa6c1aba70c6913f7fcda11`.

| Build | Version/code | SHA-256 |
| --- | --- | --- |
| ARM64 | 0.1.0-preview.22 / 100022 | `3b7d38d55e67286f8bc86c9bc7b630a0d8c5aa4f1f7bb5bc948e658898ffd7d2` |
| ARM32 | 0.1.0-headunit.17 / 100017 | `47f3295a3bdcdd4a4f2384b48aefcc2589387295f8215312439a7008d684822f` |
| Setup | 0.3.4 / 7 | `2b4485de05eaac4b3974f46c98fdc2ad98201dfc411b93a6f123b84117e3e203` |

Release assets, source archives, receipts, certificates and notes are under `target/releases/onboarding/`. Tags point to the APK source commit; catalog and documentation commits follow separately.

## Validation

- Shared request gates, original-menu navigation and precisely scoped transport-failure classification pass. Build-profile and deferred startup tests pass.
- Signed Android instrumentation covers real firmware download/check/extraction, offline retry, cancellation, backup on version change, automatic first-run return and no repeated onboarding.
- Setup UI was exercised through source-permission denial, process restart while permission was open, installation cancellation/retry, successful signed installation and automatic app handoff. Layout passed on a 1024×600 test display.
- Installer cleanup is optional and signer-gated; cancellation leaves the product usable.
- Pixel 7 received preview.22 in place. Its new Connect and start screen and existing Saab 9.250 firmware were physically verified through the original main menu in offline mode. No adapter was connected to the Pixel. ARM64 also booted on the Android emulator.
- K2401 received headunit.17 in place. Its existing firmware/preferences hashes remained unchanged. Chipsoft was detected and a fresh bench VIN led directly to native startup.
- Final K2401 measurement: **7.543 seconds** from the key-ON connection action to the observed native main-menu marker, including VIN connection and host/ADB overhead; native elapsed marker **5.495 seconds**. Poll interval 0.2 seconds. One returning-user bench run, firmware already installed, online lookup off. This meets the ten-second target for that run; it is not an installation-duration or all-device guarantee.
- Staged website download button was checked at 1280×720, 1280×600 and 390×844.

The ECM-only bench lacks OBD pin 1 / single-wire CAN. HS-CAN VIN reading succeeds; original vehicle checks can still fail on the single-wire wake step. No successful ECU software-version or DTC result is claimed. The explanation is limited to the observed SingleWire wake intent followed by Chipsoft opcode 000F/status 001D; other 001D failures are not assigned that meaning.

## Video workflow documentation

The earlier 8:37 recording remains the before-workflow evidence, with major stages documented in the local workflow review. It includes agent/control pauses and is not a normal-user timing benchmark. The new 7.543-second result measures only returning-user connection to main menu and must not be compared to the entire installation video as an installation speedup.

Private test screenshots, logs and the new Chipsoft connection recording are stored in the local `onboarding-20260919` evidence directory. Logs/video can contain a VIN and are not release assets. A newly edited, fully redacted end-to-end after-video has not been produced in this change.

## Rollout status and next steps

Local branch: `codex/streamlined-onboarding`. Local release tags: `v0.1.0-preview.22`, `headunit-v0.1.0-headunit.17`, `setup-v0.3.4`.

The existing publishing logins were restored. The branch and tags were pushed, all three prereleases published, and both APK filenames in each release downloaded from their public URLs and checked against the signed build hashes.

- [ARM64 preview.22](https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.22)
- [ARM32 headunit.17](https://github.com/djfremen/OpenSAAB-T2/releases/tag/headunit-v0.1.0-headunit.17)
- [Setup 0.3.4](https://github.com/djfremen/OpenSAAB-T2/releases/tag/setup-v0.3.4)

The Docker repository was confirmed private before pushing the website overlay. The existing Koyeb service was updated to image digest `sha256:e7041a452f4a3dbaf33b6bada55114247b199969e445a4a1df98f34c87824edd`, preserving its configuration. Deployment `b8cf9f7f-1247-4bec-a38a-315462b8c047` became healthy. Live `index.html`, `download.html`, `t2.css` and `releases.json` were verified byte-for-byte against the staged overlay. `/api/health` returned HTTP 200 with status `ok` and security service `password-required`.

The four-file website overlay and a patch from the live baseline are preserved in the website repository under `runs/onboarding-20260919/`. Existing dirty website source files were not overwritten. The external backup drive was not mounted; artifacts remain locally and on GitHub.
