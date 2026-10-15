# OpenSAAB T2 system requirements

## Before downloading the emulator

The current **OpenSAAB-T2-arm64-v8a.apk requires Android 8.0 / API 26 or newer and an Android system that supports `arm64-v8a` apps**. A 64-bit processor with a 32-bit Android installation does not qualify. An advertised Android version or a QLED display does not establish compatibility.

Android can reject an incompatible APK before OpenSAAB gets a chance to display a first-run explanation. The generic “App not installed” message is not a diagnosis by itself.

[Download OpenSAAB System Check](https://github.com/djfremen/OpenSAAB-T2/releases/tag/system-check-v0.1.0) before installing the emulator if you are unsure. This separate, Java-only APK runs on Android 5.0 / API 21 and newer, including 32-bit ARM systems. It contains no native emulator, firmware, network permission or USB communication. It does not replace OpenSAAB or change its data. Use **Copy report** to share the result. Device information remains local until you choose to share it.

The checker itself is not a 32-bit version of the emulator. It reports prerequisites for the published ARM64 release. Passing does not establish working adapter communication, firmware operation or acceptable performance.

| Requirement | Current release |
|---|---|
| Android | Android 8.0 / API 26 minimum |
| Application architecture | `arm64-v8a` must be among Android's supported ABIs |
| Setup space | Allow at least 140 MiB free for firmware setup/working copies, plus APK installation and ongoing logs/reports |
| Vehicle connection | Android USB host support, a working data/OTG cable, supported adapter and normal adapter/vehicle power |
| RAM | No established hard minimum; 1 GB devices are not validated. Low-memory performance requires testing |
| Internet | Required for firmware downloads and security-access API processing. Installed local diagnostics can work offline |
| Display | Original LCD retains 4:3 aspect ratio; head-unit landscape layout is not yet validated |

## Device evidence

- Pixel 7: project development baseline.
- Huawei Mate 20 X EVR-L29: user-reported successful public APK installation and Chipsoft DTC read. Other workflows are not yet validated on it.
- Universal_U01 reporting AC8227L, 1 GB RAM, 1024 × 600: user observed “App not installed.” AutoChips documents Cortex-A7 cores, a 32-bit architecture. If the platform identification is accurate, the ARM64 release cannot run on it. We have not retrieved its installer error or actual Android API level.

[AutoChips processor specifications](https://img.sae-china.org/resources/platform/uploadid_1612260328_WGAF.pdf) · [Arm Cortex-A7 documentation](https://www.arm.com/products/silicon-ip-cpu/cortex-a/cortex-a7)

A software update cannot turn a 32-bit processor into a 64-bit one. Supporting these devices would require a tested 32-bit emulator build; no such release is currently available.

## Development

The checker shares its requirement logic and local report with the main app. Upcoming app builds expose **Check device compatibility** in setup and **System check** on the main screen. These additions are not present in emulator preview.2.

Build the standalone checker with `python3 scripts/android/build-compatibility-checker.py` from a clean checkout, using `OPENSAAB_RELEASE_KEYSTORE` and `OPENSAAB_RELEASE_PASSWORD_FILE` for signing. The APK includes its source commit. It intentionally has no native ABI dependency.
