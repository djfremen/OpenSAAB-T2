# 32-bit load test

A separate offline experiment on branch `codex/32-bit-load-test`, installed as
`com.opensaab.loadtest` with the launcher label **32-bit load test**. It does not
replace the OpenSAAB T2 package. The only UI is a scrolling console; launching it
automatically cold-boots the emulator and stops at the verified Saab 9.250 welcome
screen. There are no buttons, adapters, CANdi firmware, network permissions,
vehicle communication, or saved emulator states.

## Acceptance criterion

At most **10,000 ms from before Android Activity launch to the verified guest
welcome screen**, measured on the physical ARMv7 SC7731E/UIS8141E head unit.
`run-load-test.py` supplies a device monotonic timestamp before `am start`,
force-stops the app before each trial, and copies the results to a private folder.
A normal launcher tap uses `Activity.onCreate` as its timing origin and says so
in the result. First launch also extracts firmware from the signed APK, checking
its size and CRC. The build manifest records SHA-256 hashes of all firmware.
Subsequent launches reuse the firmware files, but always reset and cold-boot the
emulator; they never restore initialized RAM or a previous screen.

Success requires the native process to exit successfully, its report to verify
the guest splash, the native completion marker to include Saab 9.250 and North
American Operations, and the final screen text to agree. The elapsed welcome time
and the later process/report completion time are both recorded.

## Optimization and limits

The M68k interpreter is compiled for 32-bit Cortex-A7 ARM with fat LTO and one
codegen unit. The optional `load-test` feature adds:

- Direct word/long reads for ordinary RAM and ROM, with the original byte bus
  retained at compatibility overlays, CFI status addresses, and device boundaries.
- Exact-opcode-guarded translations of busy RAM scan/test, copy, and string loops.
  Memory operations still happen and register/condition-code results are retained.
- STOP advancement only up to the next modeled timer event and observation boundary.
- Cached immutable process debug flags and fewer unnecessary console redraws.

The translated loops preserve guest instruction-clock counts and yield before
interrupt deadlines, loop exits and screen-observation boundaries. Differential
unit tests compare registers, flags, memory effects, card read counters and
subsequent interpreter execution. A complete accelerated boot is compared with
a reference boot for instruction count, final PC, and the complete LCD image.

This uses the existing research-harness boot model and its existing compatibility
behavior. It adds no firmware patches and does not skip a memory test or inject a
welcome screen. The optimization is deliberately scoped to this offline load test:
it does **not** demonstrate that CANdi or vehicle diagnostics start within ten
seconds, and it is not a fidelity-mode hardware certification.

## Build and install

Requires Rust with `armv7-linux-androideabi`, Android NDK r29, SDK platform/build
tools 36, and Java 17+. The builder defaults to the Mac SDK and Android Studio JDK
locations; `ANDROID_HOME` and `JAVA_HOME` override those defaults. Supply the
existing authorized firmware directory containing `eprom.bin`, `opsys.dwn`, and
`card.bin` (Saab 9.250). Firmware and built APKs remain private and are not checked
into source control.

```sh
CARGO_PROFILE_RELEASE_LTO=fat \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/armv7a-linux-androideabi26-clang" \
RUSTFLAGS='-C target-cpu=cortex-a7' \
cargo build --locked --release --no-default-features --features load-test \
  --target armv7-linux-androideabi --bin tech2-emu

python3 scripts/android/build-load-test.py \
  --native target/armv7-linux-androideabi/release/tech2-emu \
  --firmware-dir /path/to/private/firmware

adb -s DEVICE_SERIAL install -r target/android-load-test/32-bit-load-test.apk
python3 scripts/android/run-load-test.py --serial DEVICE_SERIAL \
  --output /path/to/private/results --runs 4 --fresh-firmware
```

`--fresh-firmware` removes only the test app's three extracted firmware files
before the first trial, so that trial includes first-launch extraction. It retains
prior reports and does not touch the main OpenSAAB app. Each following trial is a
new process and a fresh emulator reset with the installed firmware files.

Tests:

```sh
cargo test --locked --no-default-features --features load-test --lib --bin tech2-emu
cargo test --locked --no-default-features --lib --bin tech2-emu
```

Native reports and console logs are in the test app's private `files/runs/`;
`files/latest-result.json` contains the latest completed trial. The launcher
retains no USB or network permissions. This development APK is debuggable and
signed with the local Android development key.

## Measured result — 2026-09-17

On the USB-connected SC7731E head unit (four Cortex-A7 cores, 1.3 GHz, ARMv7,
SDK 27), the installed APK passed four consecutive process-cold boots:

| Trial | Firmware preparation | Verified welcome | Entire process/report |
|---|---:|---:|---:|
| Fresh extraction | 2.444 s | **9.965 s** | 10.210 s |
| Existing firmware, fresh emulator | 0.630 s | **7.695 s** | 7.930 s |
| Existing firmware, fresh emulator | 0.991 s | **9.008 s** | 9.259 s |
| Existing firmware, fresh emulator | 0.710 s | **7.848 s** | 8.056 s |

These are before-Activity-launch measurements. First-extraction margin is small;
these measurements establish this load test's result, not a worst-case startup
guarantee under arbitrary Android background load. The installed APK's SHA-256
matches the local artifact. Native reference and accelerated boots both reached
27,600,000 guest instruction ticks, final PC `0x1c1e08`, and identical complete LCD
SHA-256 `cc9c89c96e657cb2a135a16c436fb4bb31ce1cd5bc0dfcb29cfbf5d83aa542ae`.

216 load-test unit tests passed (10 ignored); 209 feature-disabled tests passed
(10 ignored). Private per-run console logs, reports, device performance samples,
screenshot, firmware provenance and APK hashes are retained under the operator's
`headunit-startup-20260917/load-test-final-validation` evidence directory.
