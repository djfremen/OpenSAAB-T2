# 32-bit interactive startup and deferred CANdi test

The separate interactive test APK restores the full Android display, keypad,
actions, firmware management and adapter screens around the fast ARMv7 cold boot.
It is a development profile, not a production release or a claim of vehicle
communication. The standalone `32-bit load test` baseline is retained unchanged.

## Physical result

On the attached SC7731E/Cortex-A7 head unit, three process-cold launches of the
final APK reached a **drawn, pixel-verified original welcome screen in 9.221,
9.251 and 8.745 seconds**. These measurements start before `am start`, include
Activity startup, and compare the actual drawn bitmap to the native verified
welcome PPM. Firmware was already provisioned; these are not power-on or first
firmware-import timings. CANdi was absent from all three startup measurements.

Package: `com.opensaab.tech2.headunit32.test`

Launcher: **OpenSAAB 32-bit interactive test**

APK: `target/android-headunit32-interactive-test/OpenSAAB-32-bit-interactive-test.apk`

SHA-256: `cee7f6172486b18a633e1d78f9f7c2c1427ba4642ed5db4d3b3f4f47300273a1`

The separate baseline APK remains SHA-256
`918aceda493a4a045787429ff35c02f7c8c9c54c7276fada017e0f6d54585405`.

Private measurements, screenshots and runtime logs:
`~/.local/share/opensaab/headunit-startup-20260917/interactive-demand-exit-final/`.
No OEM firmware is embedded in the APK or checked into this branch.

## Initialization boundary

The supported Saab 9.250 guest can navigate welcome → main menu → year → model
→ system → customer function without constructing the CANdi CPU or native
adapter backend. UART configuration, loopback and passive cable ADC state remain
available. Selecting Engine Control reaches the original communication entry,
where initialization runs once and retains the original caller and arguments.

Deferring only the secondary CPU is insufficient. The guest normally attempts
its CANdi inventory/application initialization from its welcome loop. The
experimental lifecycle hook therefore runs those same original routines at the
first communication dependency:

1. Hold the communication-entry call before executing its first instruction.
2. Construct CANdi and execute its bounded 100,000-instruction cold preparation;
   then attach a selected native backend, if any. No invented serial reply or
   external CAN completion is provided.
3. Execute original pSOS `tm_wkafter(1)` calls while the real presence interrupt
   and guest driver complete discovery.
4. Execute the original mark-attempt(8) helper and inventory/application loader,
   matching the sequence at the guest welcome call site `0x1c1f10`.
5. Require the guest loader's accepted result and application flag, restore the
   caller's registers/status/stack, and execute the initiating operation once.

The hook is restricted to the experimental feature and research harness. It
checks the exact supported entry/helper opcodes and stack range. It does not
patch guest instructions or force a successful connection/result flag. However,
it deliberately changes *when the original guest initialization routines are
called*; it is an experimental lifecycle integration, not a fidelity-mode claim.
Initialization failure stops before sending the pending diagnostic request. A
60-second host deadline bounds this setup; guest serial and live CAN timing
models are unchanged. Boot loop acceleration ends at the verified welcome.

After setup, the same NativeLink instance remains for the session. Offline
checkpoint recovery can intentionally restore a prior offline state; live
transport state is never rewound. With no adapter, the current offline CAN
register probe stops at the first attempted CAN transmit, as the eager reference
does. This is an expected test boundary, not successful ECU communication.

## Android adapter screens

Only the explicit debuggable test package adds `--candi-on-demand` and startup
acceleration. Existing Nano/Chipsoft native screens wait for the emulator's first
backend connection instead of expiring after 5–15 seconds of menu browsing.
Cancellation, emulator exit, and a 30-minute wait limit still end that wait.
Production package names keep their existing behavior.

USB permission/CDC opening and existing Chipsoft VIN preflight remain in their
existing adapter-selection flow. Native CANdi/backend construction is deferred;
this does not claim that every Android USB/preflight operation is lazy. Selecting
an adapter still starts its existing native session; seamless switching from an
already-running offline session is not implemented here.

## Validation

- 227 feature-enabled Rust tests and 219 feature-disabled Rust tests passed.
- GUI-feature test targets compiled; three native UART/checkpoint tests passed
  using a temporary link to the private CANdi fixture, removed afterward.
- Android architecture/package checks, existing request/navigation tests, and new
  deferred bridge wait/cancel/child-exit/package-isolation checks passed.
- Host navigation test asserts no early CANdi construction and exactly one guest
  initialization. All **79 outgoing serial commands** from the first diagnostic
  operation through CAN wake-up match the eager reference byte for byte.
  Sequence SHA-256:
  `7f1355f548ec296c42884f9ead0bfff9134ab204c7f5b24b29f2ed4bc5d00c7d`.
- Final APK on the physical head unit navigated through the same menus (including
  EXIT and re-entry), completed
  the original CANdi application handshake, and reached the absent-adapter CAN
  boundary. Deferred guest setup in that run took 8.237 seconds, paid only at the
  first operation, separately from app startup.
- No diagnostic adapter was attached. Actual Nano/Chipsoft/J2534 communication,
  vehicle timing, reconnects, and the other two head units still need hardware
  validation. No 64-bit phone result is inferred from these tests.

## Reproduction

Build native binaries with NDK r29, target `armv7-linux-androideabi`, no default
features, `--features load-test`, `RUSTFLAGS='-C target-cpu=cortex-a7'`, release
fat LTO and one codegen unit. Then:

```sh
OPENSAAB_BUNDLE_SUPPORT=0 python3 scripts/android/build-tech2-app.py --profile headunit-arm32-test
python3 scripts/android/test-demand-startup.py
python3 scripts/bench/test-demand-navigation.py --native target/release/tech2-emu --firmware PRIVATE_FIRMWARE_DIR --output NEW_EVIDENCE_DIR
python3 scripts/android/run-interactive-test.py --serial EXPLICIT_ADB_SERIAL --launches 3 --navigate --output NEW_PRIVATE_EVIDENCE_DIR
```

The device test intentionally requires an explicit serial and targets only the
isolated test package. Its automated navigation uses the guest mailbox; physical
touch navigation is a separate check. The APK builder rejects release signing
for this experimental profile.
