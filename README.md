# OpenSAAB T2

**Head-unit development branch:** experimental 32-bit ARM work is isolated under the explicit `headunit-arm32` build profile. The published ARM64 APK remains unchanged. See [build targets and separation rules](docs/ANDROID_BUILD_TARGETS.md).

Original diagnostic menus on Android, powered by a Rust emulator and direct USB adapters.

**First public developer preview: v0.1.0-preview.2.**

[Download the signed ARM64 APK](https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.2) · [Installation guide](docs/INSTALL.md) · [Website](https://www.opensaab.com/) · [Support on Ko-fi](https://ko-fi.com/djfremen)

**Before installing:** Android 8.0+ and **64-bit ARM Android (`arm64-v8a`)** are required. A 64-bit CPU alone is not sufficient. [Check your device / full requirements](docs/SYSTEM_REQUIREMENTS.md).

## What you can test

- Original Tech2 firmware menus with touch gestures, a collapsible keypad and console.
- Direct Chipsoft Pro USB connection, VIN/session identification, ECU information and DTC reading.
- Saved DTC reports grouped by module, user-reviewed sharing, and an optional donation link.
- Guided security-data collection and processing through the OpenSAAB API.
- First-run software download, extraction, verification and local storage; English Saab NAO is the default.

**Security access requires internet access to the OpenSAAB security-access API. It cannot be processed offline.** Installed firmware, local USB diagnostics and saved reports work without internet. Online VIN enrichment, downloads and sending email also need connectivity.

## Compatibility and limits

Android 8+ with a 64-bit ARM Android system is required. Pixel 7 is the current physical-phone baseline; a Mate 20 X owner has also confirmed installation and a Chipsoft DTC read. Head units are experimental and have not been validated for this release; QLED describes the display and does not establish Android ABI or USB-host support. Adapter screens currently use a portrait layout.

| Adapter / feature | Preview status |
|---|---|
| Chipsoft Pro | Current test path; original-firmware ECU information, DTC and selected security/configuration workflows previously exercised on project vehicles |
| VCX Nano | Experimental; private Android initialization, security collection and SPA Add milestone verified on Pixel 7 / firmware 1.9.4.2; public APK coverage remains separate |
| MDI / other adapters | No supported Android backend yet |
| Live data and module programming | Incomplete coverage; success on one vehicle is not general compatibility |

Use **Run in emulation mode** to explore menus without connecting to a vehicle. Start physical testing with VIN/ECU information and DTC reading. A displayed DTC list is not proof every module was scanned. This preview does not promise universal module programming or configuration recovery.

### Nano development checkpoint — October 5, 2026

The Nano backend now establishes the transient PASSTHRU session before opening
channels, using the OpenVCX-derived `GetInfo → A0 → 84 → A0 → A1 → A2`
sequence and two fresh DH exchanges. A software-reboot test on Pixel 7 obtained
a fresh VIN without Windows priming. Removing both USB and OBD power has not
yet been tested; a software reboot does not qualify that physical cold start.

The owner-operated private `nano-init.4` app collected fresh security data,
processed it through OpenSAAB and imported only the SSA card region. The
original Saab menus then reported **Checking Security Access OK**, **Adding
Unit OK** and **Add / Remove ECU finished** for SPA (Saab Parking Assistance)
on one 2004 Saab 9-3 Sport. API processing/import alone is not a vehicle access
grant. The default read profile, seed collection and private full native control
remain distinct.

Private `nano-init.5` uses the existing shared background keypad queue and
event-driven LCD pump in the Nano firmware screen. On the installed Pixel 7,
nine menu keys reached the mailbox in 0–2 ms; 19 distinct guest display frames
reached ImageView assignment in 5–68 ms (median 11 ms). These are app transport
measurements, not a touch-to-ECU comparison. The menu-only check sent zero
diagnostic transmissions and stopped with both channels and USB closed.

[The sanitized receipt](docs/android/ANDROID_NANO_SECURITY_SPA_2026-10-05.json)
pins the tested artifacts, results and remaining limits. These changes are
source development work; the published preview.45 APK does not contain them.
Other architectures, adapters and vehicles require their own qualification.

## Software and source

The APK includes `eprom.bin`, `opsys.dwn` and `candi.bin` as separately identified original support files. It does not include the larger Saab PCMCIA card. Choose/download a supported card during setup, or import one locally. Windows language DLLs do not translate Saab card images.

Original OpenSAAB source uses [MPL-2.0](LICENSE). Firmware and other third-party materials retain their own terms; the project license does not grant rights in them. See [licensing scope](LICENSING.md) and [dependency notices](THIRD_PARTY_NOTICES.md).

The OpenVCX-derived Nano initializer retains LGPL-3.0-only and its accompanying
source attribution and license texts. The surrounding OpenSAAB source remains
MPL-2.0.

The APK is built from this source tag with the pinned support inputs identified by hash. Build instructions, checksums, certificate identity and a build receipt accompany the release. The release-signing key is private; independent byte-for-byte reproducibility has not yet been established. [Build instructions](ANDROID_STUDIO.md).

This is a clean source export, not a publication of private research history. Legacy vehicle-specific host-command fixtures use synthetic identities and are not supported public workflows. Use the original firmware menus for vehicle tasks. Developer auto-start intents are disabled in non-debuggable builds.

## Community

[Questions and ideas](https://github.com/djfremen/OpenSAAB-T2/discussions) · [Report an issue](https://github.com/djfremen/OpenSAAB-T2/issues/new) · [Contribute](CONTRIBUTING.md)

Include app version, device/Android version, adapter model, menu path and expected versus actual behavior. Use **Report issue** to prepare a support report and review it before sharing. Do not publish VINs, seeds/keys, SSA files, raw captures, firmware, passwords or private screenshots. Reports are shared by the user, not silently uploaded.

Donations are voluntary and do not unlock features. A Google Play closed test is not running. Independent community project; not affiliated with Saab, GM or adapter manufacturers.

### Adapter implementations

Adapter-specific USB code is organized into [Chipsoft Pro, VCX Nano and shared transport buckets](docs/adapters/README.md). That document describes detection, initialization ownership and tested coverage.
