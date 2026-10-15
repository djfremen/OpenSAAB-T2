# Android recovered Rust boot — October 14, 2026

`OpenSAAB 0.1.0-findings.7 ARM64`, versionCode `100078`, is a private Android
development preview. The ARM64 APK is built from the Android integration checkout
at `/Users/mini4/Documents/Projects/android-findings-20261014`. The original Android
workspace, controls, setup and adapter choices remain the common interface.

## Changed runtime

The recovered core adopts the canonical Rust CFI program/erase handling, OEM LCD
address decoding, ENTER encoder and recovered pSOS host kernel from Mac core
`0.2.25-recovered-core.4`. The task/kernel code is shared source; the Android
native-menu publication, empty capture option and MX+ byte effects are retained.

Run without an adapter now launches that core in fidelity mode, with the selected
EPROM/card and the existing capture/Data Display services. It does not restore
`opsys.dwn`, activate research patches or start CANdi. Evidence records
`offline_native`; existing `offline_research` evidence remains readable.

MX+ retains its existing research startup pending separate live qualification.
Chipsoft/Nano and both retained MDI native payloads are byte-identical to the
preceding `.6` package. This change does not silently migrate their transports.

## Exact Android execution

The APK was installed in place on `emulator-5554`, an Android API 36 ARM64 AVD.
The displayed application label includes the full version and architecture.
Package metadata reads back `0.1.0-findings.7` / `100078`, and pulled APK hashes
match the packaged artifact. Android Studio Run also built, installed and
launched a test-only variant. Its only payload difference is Android's
`android:testOnly=true` manifest marker; all Java/native executables, resources
and build metadata match. Both exact APK variants passed the installed native
boot/navigation test. Use the ordinary CLI APK for private installation.

The AVD's previously selected, unchanged **Saab 148.000 English** image is the
fixture. Its SHA-256 is
`e2c4304323d456af3516cea081c4a324448b47f9986328cecf10672ae6c2d4e1`.
The native welcome is verified at 24,000,000 instructions, around 1.9 seconds.
The installed test follows genuine ENTER → Main Menu → F0 / Model Year(s),
DOWN → 2 of 15, UP → 1 of 15, EXIT → Main Menu, and clean session Stop.
The host kernel is active with four tasks, context switches and no kernel error.
The original input hashes are unchanged before and after; no adapter is opened.

Signal version 3 and actual Data Display handshake hooks are observed, with zero
fabricated live rows. The signal-version restriction continues to apply only to
OEM DTC Display; existing Rust DTC reading and saved reports remain independent.

Stop intentionally reports native incomplete / exit 3 while the Android session
records `user_stop` and `forced_stop=false`. That is an expected operator stop,
not a failed guest boot. The initial test predicates used the Mac's 9.250 fixture
and then admitted the firmware's transient 0.000 welcome. Those test attempts are
retained; the final test waits for the selected firmware's actual version.

## Verification and artifacts

- 543 host checks pass, zero fail, 18 ignored for this exact recovered source with
  `load-test,embedded`, including the Android native-menu/capture extensions.
- Gradle APK payload checks and Android Lint pass.
- Exact installed native boot/menu/Stop, MCP authentication/revocation and
  offline evidence/privacy instrumentation pass.
- APK SHA-256:
  `a560396cc48041d41194b8bfe88b964a377e1e0d3b189ec054317c11dd7224fe`.
- Recovered ARM64 native worker SHA-256:
  `b37d68726d5fec32b693e5e9ea9d0422b554c1dcff73297aad09baf8dbf5e3d2`.
- Studio test-only APK SHA-256:
  `b41eff344eb2079c4c131a7cc56639139cb3d9e1753c667b6ddbf8384d8f4fe8`.
- Versioned APK, installed receipts, logs, exact session evidence, source archive
  and manifest are retained at
  `/Users/mini4/.local/share/opensaab/android-recovered-boot-20261014/0.1.0-findings.7/`.
- The preceding AVD APK and private app data were preserved before upgrading.
  Historical `.6` Android and `.4` Mac artifacts remain unchanged.

Build with the existing approved support-file profile and JDK 21:

```sh
JAVA_HOME=/path/to/jdk21 ./gradlew :app:verifyDebugApk :app:lintDebug --no-daemon
```

The packaged native worker uses the installed Android NDK r29. This preview APK
is debuggable and signed with the project's existing private preview identity.
There is no new public release or updater change. Source compilation on a host
does not qualify another installed ABI, physical phone or adapter.

## Remaining scope

Physical Pixel/MX+/ECM testing is pending: only the ARM64 AVD is currently
connected through ADB. ARM32 packaging and installed tests, adapter-specific
native-kernel migration, all firmware/language versions, live capture/data and
full common-interface qualification remain separate work. The 17 pre-existing
global baseline gate errors are retained; this checkpoint promotes no uniform
release or inherited hardware result.
