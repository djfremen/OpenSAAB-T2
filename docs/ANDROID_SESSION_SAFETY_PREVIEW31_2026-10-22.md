# Android ARM64 session safety — preview.31

Historical status: **rejected before installation** on October 22. A subsequent
source review found that a queued VIN-success callback could start firmware after
Stop during the worker-to-UI handoff gap. Its successfully built artifacts and
receipts remain immutable; no installed or physical qualification is claimed.
The successor is [preview.32](ANDROID_SESSION_SAFETY_PREVIEW32_2026-10-22.md).

This candidate follows the October 22 four-adapter review of preview.30.
It is prepared for another owner test round. Software checks, package identity,
installed verification and physical qualification are separate records.

## Changes

- Chipsoft operator Stop uses a bounded native stop request and keeps the USB
  bridge available for the existing cleanup handshake. New diagnostic traffic is
  rejected after Stop; forced fallback and failed cleanup remain visible.
- Shared process termination waits for actual death before releasing firmware
  ownership. An asynchronous kill request is insufficient evidence of release.
- All three native firmware engines use the same outcome policy. A clean manual
  operator Stop reports stopped, not a completed diagnostic workflow. A prior
  adapter failure or known cleanup failure remains a failure. Final native
  reports follow joined cleanup; host release is recorded separately.
- The rebuilt MDI JNI/USB carrier requires both clean transport teardown and a
  successful child exit for its completion result. Operator-stop intent cannot
  turn an actual native failure into success.
- Continue firmware reflects running, cleanup and stopped states. The six
  MainActivity actions that open new connections are hidden during active
  sessions. Working Chipsoft original-firmware navigation remains available.
- RFCOMM errors retain the observed phase, duration, original platform error and
  write-attempt count. Failed opens do not infer sleep or an HCI status. There is
  no automatic connection retry, adapter reboot or vehicle-action replay.
- Native reports count UART timeouts once per trace stream and retain original
  POST failures after navigation. Manual PPM capture keeps the latest frame,
  final screen and fault evidence; console text retains the navigation history.
- A tracked build recipe rebuilds the root/MX, MDI and recovered firmware engines
  plus the MDI carrier: four crate builds supply five packaged native entries.
  The three unchanged Nano/Chipsoft probe and connection workers have explicit
  hashes and original source provenance. Dependency hashes, toolchain inputs,
  current and historical matching source, and all eight
  packaged native components accompany the candidate.

The contract snapshot is
[session-lifecycle-contract.json](../android/tech2-app/session-lifecycle-contract.json).
Other platforms remain pending binding, package and physical verification.

## Deliberate limits

Keeping an MX+ socket through security processing needs a new ownership contract.
It is not enabled by this candidate. The engine must still stop before exclusive
card import; exact VIN checks and fresh continuation remain. Compare HCI captures
of OpenSAAB and the official app under measured matching idle/sleep conditions
before diagnosing the observed 12.8-second failures. Set up snooping only between
runs, because enabling it can require restarting Bluetooth.

ERAM POST behavior is retained as observed; no forced Pass or memory-model
workaround is introduced. UART timeouts are not blanket-classified as harmless.
Stopped, imported security data and firmware completion screens do not establish
current ECU authorization or independently verified operation/readback.

This is a bounded lifecycle consolidation through shared outcome, cleanup and
process-ownership helpers. Adapter-specific protocol controllers remain separate.
Their complete migration into one controller requires its own qualification.

No beta hardware or vehicle safety qualification is claimed by a build, emulator
test or source review. Existing authority profiles, electrical routing limits,
internal authentication, original confirmations, cancellation and SSA validation
remain. There is no operator password.

## Owner test round

For every independent adapter run:

1. Record the installed full version/architecture, APK and native hashes,
   device/adapter identity and exact card. Back up the card, explicitly reset the
   selected working card, and verify INIT_AUTH/hash before starting.
2. Connect in the selected supported mode, complete boot and verify fresh VIN.
3. Run original Vehicle Check / Read faults; retain module coverage, codes,
   missing responses and warnings.
4. Complete the common service/security collection, processing, validated import
   and continuation path, following the original physical prompts.
5. Retain the completion screen and required post-operation/readback evidence.
6. Stop and verify final native protocol cleanup, actual host transport release
   and absence of residual workers. A reported stopped process is not a pass.
7. Follow the agreed repetition gate, including measured MX+ sleep/reconnect or
   USB replug as applicable. Preserve this run's data across retries/import/
   reconnect; reset only for a new independent run.

All four adapters retain the same bar. Shared lifecycle/native changes require
the full path on all four. Chipsoft's general Stop is the first regression to
target; Nano/MDI are useful controls, and MX+ still needs its reconnect result.
Physical results from preview.30 remain historical and cannot qualify preview.31.

## Build

See [the tracked preview build procedure](ANDROID_PREVIEW_BUILD.md).
The final external candidate manifest records the clean source commit, APK hash,
all native hashes, source/toolchain/input receipts and verification scope.
Historical preview packages and research evidence remain unchanged.

