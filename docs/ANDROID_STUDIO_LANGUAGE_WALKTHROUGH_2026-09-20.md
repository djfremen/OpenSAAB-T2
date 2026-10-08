# OpenSAAB T2 — Android Studio installation and languages

Recorded September 20, 2026. Full silent MP4: `OpenSAAB-Android-Studio-install-and-languages.mp4` (about 6 minutes 55 seconds, 600 × 1024, H.264). Chapter markers are included; timings below are approximate.

## Environment

Android Studio SDK emulator `OpenSAAB_SetupTest`, Android 16 / ARM64. The installed app is release-signed **0.1.0-preview.23**, version code **100023**, built from commit **afea880**. The previous app was uninstalled before recording; no old installer package was present. The APK was downloaded in Chrome from a clearly labeled local preview page and installed through Android's package installer, not through `adb install`.

APK SHA-256: `c905ee8e2ec5bbc05682fed62338c4a2f650ec7cd443104817fe916cbc0f6691`.

## Recorded workflow

| Time | Interaction |
| --- | --- |
| 0:00 | Local preview download page; ARM64 build and test status explained |
| 0:20 | APK download, including Chrome's local HTTP download warning |
| 1:10 | Open the downloaded APK; permit installation from Chrome |
| 2:00 | Play Protect unfamiliar-app prompt and Android installation |
| 3:10 | Open OpenSAAB, choose Deutsch, download and verify Saab 148.000 |
| 3:50 | Connect and start reports no supported adapter; choose offline emulation |
| 4:15 | German splash/main menu, vehicle-year menu, ENTER, scrolling and EXIT |
| 5:05 | Stop emulation; change to English 148.000; download, verify and activate |
| 6:20 | Start again and navigate the English diagnostic menus |

The video retains the installation prompts and waiting time. The final file uses a constant 20 fps for playback compatibility; the original variable-frame-rate capture is retained in `android-studio-language-install-20260920/clean-install-source.mp4`.

## What this establishes

- The signed ARM64 test APK installs and launches through Android's installer.
- First setup downloads the selected German image, verifies it, and returns to the app automatically.
- Original diagnostic menus render in German and respond to navigation controls.
- Language selection reopens on the installed German version. Switching to English completes and the English menus work. A read-only signed instrumentation check confirmed both original image hashes, the English active-language metadata, and a checksum-verified German working-card backup.
- An absent adapter is reported explicitly. Offline mode remains labeled as having no vehicle connection.

The prior physical Pixel tests already exercised the real language selectors, startup cancellation, firmware backup/cancellation/recovery, and offline boot of all ten runnable images across nine languages. The user selected the Android Studio emulator as the replacement for the interrupted Pixel installation recording.

## Limits / observations

No adapter or ECU was attached. This does not validate a live VIN/DTC read, vehicle communication in every language, or the new ARM32 APK on a physical head unit. The installed build and the local preview page have not been published to the production website. OpenSAAB app controls remain English; automatic menu shortcuts require a recognized English card.

The German splash showed `Software Version: 0.000` during this run, while the verified installed image was labeled 148.000 and its German menus worked. That splash-version display needs separate investigation; it is not evidence of a different downloaded version.
