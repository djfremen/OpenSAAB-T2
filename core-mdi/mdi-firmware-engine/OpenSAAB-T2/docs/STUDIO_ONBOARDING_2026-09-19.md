# Pixel feedback and Android Studio verification — September 19, 2026

## Changes

The idle home screen now offers **Run without an adapter** directly below **Connect and start**. Both launch buttons disappear during offline emulation. Missing firmware reopens guided setup; firmware installation and security processing still exclude concurrent startup.

On the first firmware frame, a brief controls guide explains swipe, hold for ENTER, EXIT and Keypad. It is a dismissible overlay, not a setup dialog, and does not resize the LCD. Its touch surface is separate from the firmware gesture surface. Dismissal is remembered across activities/restarts; full controls help remains available. Offline, Chipsoft and Nano screens share this behavior.

## What Studio found

1. **Adapter source lookup:** the custom builder compiled relocated classes, but Android lint could not resolve the manifest's Chipsoft/Nano activities because their paths no longer matched their Java package. Each adapter now has its own conventional source root under `android/adapters/`, containing `com/opensaab/usb/`. Gradle, lint and the custom APK builder agree. Existing package/component names and command gates are unchanged.
2. **Gradle JVM mismatch:** bundled JBR 25 cannot compile this Gradle 8.13 project's build script. Selected the installed Temurin 21 for the new Studio workspace. The IDE itself continues using its bundled runtime. No Gradle/AGP upgrade was needed.
3. **GUI build environment:** Studio does not necessarily inherit shell variables. Added an ignored `local.properties` setting, `opensaab.supportRoot`, so Run can find the existing hash-checked support inputs. Explicit `OPENSAAB_SUPPORT_ROOT` still takes precedence.
4. **Separate processes:** the UI and `libtech2_emu.so` run separately. A Java-only memory view omits the actual emulator. One idle-menu sample was 40,445 KiB PSS for the app and 38,389 KiB for the native process; these are AVD snapshots, not physical-device limits, startup benchmarks or leak tests.

## Verification

Environment: M4 Mac mini; ARM64 API 36 Google Play AVD `OpenSAAB_SetupTest`, using Swiftshader; landscape 1024×600 and portrait 393×852 at 160 dpi. No physical device was changed.

- Android Studio **Run app** built, installed and launched the development APK on the explicitly selected disposable AVD. The existing research workspace remains in its own Studio window.
- `:app:assembleDebug`, `:app:lintDebug`, `:app:verifyDebugApk`: passed using JDK 21. Lint: **0 errors, 155 warnings**; 123 warnings concern hardcoded/untranslated UI text. Other findings include locked orientations, backup policy, draw allocations, older APIs, constructors, supported architectures and tool versions. Warnings remain visible; no blanket suppressions/baseline were added.
- ARM32 optimized native executables and development APK rebuilt; correct ARMv7 payload verified. This is build verification, not ARM32 execution on the ARM64 AVD.
- Host request-gate suite: passed, including routing, exact adapter bytes and separation of adapter implementations.
- Device `session-layout`: passed; visible offline action, persistent EXIT/actions and synthetic phone/head-unit dimensions.
- Device `keypad`: passed; first-frame guide, persistent dismissal/recreation, unchanged LCD size, guide touches/long press emit no keys, existing 23 key mappings and gesture cancellation.
- Device `firmware-download`: passed on clean app data; direct offline action recovers missing setup, initial firmware choice, offline failure/retry, actual HTTPS downloads and hash verification, automatic return, version-switch backup, cancellation preserves installed software, no repeated onboarding.
- Device `startup`: passed; ignition instructions and optional lookup disclosure, cancellation queues no USB operation, synthetic vehicle states and credential cleanup.
- Actual offline engine: Saab 148.000 splash → ENTER → Main Menu → Diagnostics/model years → swipe selection → EXIT to Main Menu → Stop. No synthetic firmware screen or vehicle reply was substituted. The 148.000 card was left installed by the version-switch test; the default download remains NAO 9.250.

The APK was installed directly by the developer tools. This does not repeat the Pixel browser/Play Protect installation video, and does not measure a customer's installation duration. USB permissions, Chipsoft detection, Nano initialization, CAN timing, ECU information, DTC and security operations still require physical tests.

## Shorter installation path

**Recommended first rollout:** make the official signed ARM64 main APK the direct phone download; keep a clearly labelled separate experimental ARM32 download and optional Setup for people who need device selection. The existing main app already owns firmware selection, verification and first-run recovery, all exercised above without Setup installed. This removes one APK installation, one separate installer source-permission step and later installer cleanup. Android installation approval and any Play Protect scan remain.

**One universal APK is a separate follow-up.** It can package both native ABIs, but the current 32-bit product has its own package, data, provider authorities and update channel. `DemandStartup` and `AppBuildProfile` currently infer the 32-bit behavior from that package. Before a universal build, select startup behavior from the actual packaged/selected native ABI, preserve optimized ARMv7 compiler features, validate all native executables, define an explicit migration for existing headunit32 installations and verify updates on both devices. Do not infer the Android application ABI from a marketing CPU label or silently remove the old package/data.

No universal APK, website download change or new public release was published during this validation. The changed source and development builds are ready for review; published preview.22/headunit.17 remain separate from these tests.

## Evidence

Private local evidence: `~/.local/share/opensaab/studio-onboarding-20260919/` contains numbered screenshots, input timeline, a bounded landscape recording, offline session logs, separate app/native memory snapshots, crash buffer and build/test logs. The recording covers part of the walkthrough; screenshots retain the later selection/EXIT/stop steps. It is not a speed benchmark. No firmware images or private session logs are committed here.
