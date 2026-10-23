# OpenSAAB T2

**Head-unit development branch:** experimental 32-bit ARM work is isolated under the explicit `headunit-arm32` build profile. It has a separate APK and release schedule. See [build targets and separation rules](docs/ANDROID_BUILD_TARGETS.md).

Original diagnostic menus on Android, powered by a Rust emulator and selected USB or Bluetooth adapters.

**Public Android development preview: v0.1.1-preview.40 ARM64 (`OpenSAAB_64_22OCT26_v0.1.1-preview.40.apk`, versionCode `101040`). Known issues remain; this is not a qualified stable release.**

[Download the signed ARM64 APK](https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.1-preview.40) · [Release naming](docs/RELEASE_NAMING.md) · [Installation guide](docs/INSTALL.md) · [Website](https://www.opensaab.com/) · [Support on Ko-fi](https://ko-fi.com/djfremen)

## Android 0.1.1-preview.40 — Full-session developer preview, October 22

Normal MX+, Chipsoft Pro, Classic MDI and GM VCX Nano connections retain their existing Full-session bindings, without the old Read-only/Full picker or an inherited Read preference changing normal entry. Original confirmations, ECU authorization, cancellation and SSA integrity checks remain. No operator password is required.

On the exact signed APK with a 2004 Saab 9-3 Sport (9440), Chipsoft completed fresh security collection/import and showed service-reset completion with matching oil-life/day readback. Its ordinary DTC scan issued 20 requests across 19 module screens, stopping at the final LOCK checkpoint before the final CIM read. Changing T8 traffic and one same-process warm reconnect were observed. MX+ separately reached key-status and Read Values screens.

**Known issues:** Chipsoft normal Stop reports protocol cleanup failure despite confirmed Android USB release. MX+ reconnect reliability and complete ordinary DTC coverage remain unresolved. An implausible airflow value and unnamed row need investigation; changing traffic does not validate numeric accuracy. Exact preview.40 MDI/Nano golden-path repeats remain pending. Earlier results do not qualify this APK or complete beta hardware-protection behavior.

Stop diagnostics before updating, keep important backups, and install over the same-key Android app. Same-package reinstall preserved 675 exported evidence files; direct preview.98 migration was not physically tested. Confirm **OpenSAAB 0.1.1-preview.40 ARM64** after installation. Retain evidence and stop the workflow if cleanup fails.

[Release notes, APK, exact source and native-component archive](https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.1-preview.40) · [Download and install guide](https://www.opensaab.com/static/t2/download.html) · [Testing feedback](https://discord.gg/FxnFe8vQht)

Exact build source: `c2e38ff69979d404b22457de709381a6fb8eb017`, retained on `android-v0.1.1-preview.40-source` and in the matching-source archive. APK SHA-256: `a84d2e9d1df9fa617f2629b6d0d95405696ca3c403c2de9ab6dfdb36f994fd0c`. All eight native payloads, source pins, dependency inputs and notices are retained. Other platform channels remain independent.

## Mac 0.2.28-mxplus.5 — Intel and Apple silicon, October 20

Both Mac architectures have matching developer previews with card-derived security-transfer recognition, the existing USB backends, a read-only OBDLink MX+ Bluetooth route and optional local MCP. No operator password is required; explicit processing consent, cancellation and response/SSA integrity checks remain. MX+ clearing, security collection/processing and programming are unavailable. MCP is off by default and requires explicit local enablement.

[Intel DMG](https://github.com/djfremen/OpenSAAB-T2/releases/download/macos-v0.2.28-mxplus.5/OpenSAAB-0.2.28-mxplus.5-macos-x86_64.dmg) · [Apple silicon DMG](https://github.com/djfremen/OpenSAAB-T2/releases/download/macos-v0.2.28-mxplus.5/OpenSAAB-0.2.28-mxplus.5-macos-arm64.dmg) · [Release notes, exact matching source, manifests and hashes](https://github.com/djfremen/OpenSAAB-T2/releases/tag/macos-v0.2.28-mxplus.5) · [Installation and test scope](https://www.opensaab.com/static/t2/download.html#desktop)

Both exact native installed apps passed full identity, payload/signature, software self-test, all ten generated catalog-card prompt cases and Dutch offline firmware menu/clean Stop checks through packaged MCP. **Fresh physical MX+/USB vehicle tests and complete non-English security collection/service processing/SSA import remain pending.** Generated layouts and historical Intel MX+ key-status results do not qualify these new DMGs. macOS 12+, ad-hoc signed and not notarized. Stop diagnostics and quit the old app before installing the fully versioned new app alongside historical versions; updates remain manual.

The exact Mac source is commit `efad90e5bb06c54fb99770396f71a13c47066e6d`, linked from the release. This default branch remains the Android source line. Android, Windows and iOS retain separate versions and tests; MX+ is not yet released across every app version. Full parity remains unfinished.

## Corrected Android and Mac developer previews — October 16

We caught an error while checking the previews and corrected it. Sorry for the confusion.

**Android 0.1.0-preview.98 ARM64** is available with exact matching source, separately pinned later test source, manifests and checksums. It removes the operator password step and fixes the native/manual false splash deadline after a verified translated welcome. Exact signed-package emulator firmware/Stop, recovered-core identity, MCP, reporting and real Rust-panic checks passed. The .97-to-.98 upgrade preserved existing data. Physical Lenovo/Chipsoft/MDI/Nano/vehicle and live security-service checks remain pending. [Release and test limits](https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.98).

**Mac 0.2.27-preview.3 for Apple silicon and Intel** removes the operator password step, retires the menu contract entry and clarifies consent wording. Apple silicon passed 289 installed assertions, security-worker checks and original Swedish 148 offline navigation/clean Stop. Both installed packages passed payload/hash/architecture/signature checks. Intel runtime remains pending; older Intel preview.7 live results do not qualify these new packages. Physical vehicle/adapter and live service/SSA qualification remain pending. macOS 12+, ad-hoc signed, no hardened runtime, not notarized. [Both Mac downloads and exact matching source](https://github.com/djfremen/OpenSAAB-T2/releases/tag/macos-v0.2.27-preview.3). Updates remain manual from the [download page](https://www.opensaab.com/static/t2/download.html#desktop).

Windows [0.2.18-preview.5](https://github.com/djfremen/OpenSAAB-T2/releases/tag/windows-v0.2.18-preview.5) remains a separate portable experiment; 0.2.17 preview.1 is its earlier recommendation. ARM32 0.1.0-headunit.17 and personal Mongoose 0.1.1-preview.3 retain separate scope. MCP/recovered-engine and other desktop experiments are separate. Full parity remains unfinished.

**Before installing:** Android 8.0+ with a 64-bit ARM Android system is required. Install over the same-key app to preserve data, and retain backups. Mac: stop diagnostics, then copy the fully versioned app alongside historical builds. [All channels and exact hashes](https://www.opensaab.com/static/t2/download.html#current-channels).

## What you can test

- Original Tech2 firmware menus with touch gestures, a collapsible keypad and console.
- Direct Chipsoft Pro USB connection, VIN/session identification, ECU information and DTC reading.
- Saved DTC reports grouped by module, user-reviewed sharing, and an optional donation link.
- Guided security-data collection and processing with explicit consent and no operator password; response validation and SSA-only import checks remain.
- Experimental Classic MDI USB transport in the main app, without a running Windows relay or standalone helper. Automatic classic-MDI initialization retrieves the adapter identity; no user connection-profile import is required for the tested route.
- First-run software download, extraction, verification and local storage; English Saab NAO is the default.

**Security processing requires internet access and explicit data-sharing consent; no operator password is required.** The existing public fixed primary-service header is a compatibility value, not caller authentication or endpoint abuse protection. Optional Bojer fallback requires permission and never receives that header. Processing cannot run offline. Installed firmware, local USB diagnostics and saved reports work without internet. Online VIN enrichment, downloads and sending email also need connectivity.

## Compatibility and limits

Android 8+ with a 64-bit ARM Android system is required. Pixel 7 is the current physical-phone baseline; a Mate 20 X owner has also confirmed installation and a Chipsoft DTC read. Head units are experimental and have not been validated for this release; QLED describes the display and does not establish Android ABI or USB-host support. Adapter screens currently use a portrait layout.

| Adapter / feature | Preview status |
|---|---|
| Chipsoft Pro | Exact preview.40 service reset/readback, ordinary module reads, T8 traffic and one warm reconnect observed; normal Stop reports protocol cleanup failure despite host USB release |
| OBDLink MX+ | Exact preview.40 key-status and Read Values screens observed; intermittent connection failures and incomplete ordinary DTC scanning remain unresolved |
| VCX Nano | Included with its existing Full-session binding; earlier vehicle results retain their original build scope; exact preview.40 golden-path repeat pending |
| Classic GM MDI | Included with native USB and its existing Full-session binding; earlier vehicle/USB-release results retain their original build scope; exact preview.40 golden-path repeat pending; MDI2 unqualified |
| Live data and module programming | Numeric accuracy and wider coverage unqualified; implausible airflow/unnamed row under investigation; no dependable programming claim |

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
pins the tested private artifacts, results and remaining limits. Preview.56
publishes these source changes in a signed nondebug ARM64 beta. Its exact-package
checks are recorded with the release; the private SPA Add result remains separate.
Other architectures, adapters and vehicles require their own qualification.

Thanks to **[Erik Fuller (erik683)](https://github.com/erik683)** and
**[OpenVCX](https://github.com/erik683/OpenVCX)**. We adapted his published Nano
initialization code into the Rust backend; that work supplied the missing
session handshake. The complete attribution, original copyright and LGPL/GPL
texts accompany both the APK and matching source.

### MDI main-app milestone — October 5, 2026

The owner-operated Pixel7 `mdi-main.4` app collected fresh native security data,
processed and imported it, then completed original SPA Add through **ECU finished**.
Service read the key as **Working (Added)**, with four programmed CIM keys and
433MHz. That is a key-status read, not a new key-programming claim. All eight DTC
list positions appear in the console; the saved report captured only four.

Preview.62 packages the same six native payloads in a signed nondebuggable ARM64
app and fixes report dismissal plus manual/stale security-data workflows. Read
[the MDI guide](docs/adapters/MDI_ANDROID_PREVIEW.md) before trying it. No adapter
credential is bundled: import your own connection profile locally. The configured
Classic MDI fixture does not establish MDI2 or general adapter compatibility.
The 128MiB structured-recorder boundary can end a session; report completeness,
ending-state propagation and longer uninterrupted sessions remain tracked gaps.

The [reviewed milestone receipt](docs/android/MDI_OWNER_MILESTONE_2026-10-05.json)
keeps owner-development proof separate from exact public-package checks. Raw VINs,
seed/key material, API payloads and firmware/context backups remain private.

## Software and source

The APK includes `eprom.bin`, `opsys.dwn` and `candi.bin` as separately identified original support files. It does not include the larger Saab PCMCIA card. Choose/download a supported card during setup, or import one locally. Windows language DLLs do not translate Saab card images.

Original OpenSAAB source uses [MPL-2.0](LICENSE). Firmware and other third-party materials retain their own terms; the project license does not grant rights in them. See [licensing scope](LICENSING.md) and [dependency notices](THIRD_PARTY_NOTICES.md).

The OpenVCX-derived Nano initializer retains LGPL-3.0-only and its accompanying
source attribution and license texts. The surrounding OpenSAAB source remains
MPL-2.0.

The APK is built from this source tag with the pinned support inputs identified by hash. Build instructions, checksums, certificate identity and a build receipt accompany the release. The release-signing key is private; independent byte-for-byte reproducibility has not yet been established. [Build instructions](ANDROID_STUDIO.md).

This is a clean source export, not a publication of private research history. Legacy vehicle-specific host-command fixtures use synthetic identities and are not supported public workflows. Use the original firmware menus for vehicle tasks. Developer auto-start intents are disabled in non-debuggable builds.

## Community

[Join the openSAAB Discord](https://discord.gg/FxnFe8vQht) for releases, help, testing feedback and development. Start in **#welcome** and follow **#releases** for new builds.

[Questions and ideas](https://github.com/djfremen/OpenSAAB-T2/discussions) · [Report an issue](https://github.com/djfremen/OpenSAAB-T2/issues/new) · [Contribute](CONTRIBUTING.md)

Include app version, device/Android version, adapter model, menu path and expected versus actual behavior. Use **Report issue** to prepare a support report and review it before sharing. Do not publish VINs, seeds/keys, SSA files, raw captures, firmware, passwords or private screenshots. Reports are shared by the user, not silently uploaded.

Donations are voluntary and do not unlock features. A Google Play closed test is not running. Independent community project; not affiliated with Saab, GM or adapter manufacturers.

### Adapter implementations

Adapter-specific USB code is organized into [Chipsoft Pro, VCX Nano and shared transport buckets](docs/adapters/README.md). That document describes detection, initialization ownership and tested coverage.
