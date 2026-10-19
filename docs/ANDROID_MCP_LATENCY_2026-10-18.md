# Android MCP latency candidate

Next candidate: **OpenSAAB 0.1.1-preview.4 ARM64**, version code 101004.
This is unpublished work following the Fire 0.1.1-preview.1 adapter tests.
Those tests and the installed Fire application retain their original identity.

The Python bridge reuses the last confirmed state during exact-label navigation,
removing two redundant state requests per path step. Endpoints advertising
`wait_state_v1` provide a bounded, event-woken transition wait. Older app versions
retain polling. A wait observes input consumption, input idle and changed menu
revision; it never sends a key. A missing reply or timeout never repeats a key.

Android inspections use immutable, file-observed native-menu snapshots. Every
command still rereads the current native menu before its existing session,
revision, manual-generation and consumed-input checks. Atomic native publication
wakes the waiter. LCD and menu consumers share one directory watch, so closing
one consumer cannot revoke the other. Missed events have a once-per-second
snapshot recovery; non-observing owners have a bounded wait fallback. The wait runs
outside the UI thread with one active waiter. A bounded four-client executor
allows inspection, health and Stop while waiting, and buffered request reads
avoid per-byte socket reads. Disabling/backgrounding revokes the endpoint,
closes its connections and stops its menu observers. No adapter policy, native
CAN command, security consent or physical prompt changes.

Use the bridge as a **long-running MCP stdio server**. Configure its command as
`python3`, with these arguments (replace each placeholder):

```text
/absolute/path/opensaab-android-mcp.py
--serial
EXPLICIT_PAIRED_ADB_SERIAL
--token-file
/private/path/pairing-code.txt
```

Enable **AI control (MCP)** in the foreground app and store its displayed pairing
code in a file with mode 0600. The bridge retains one ADB forward until process
exit. Repeated `--call` processes recreate that forward and are intended for
one-shot diagnostics. Keep one bridge owner and use `opensaab_android_navigate`
for a route of at most twelve exact visible labels. Native consent dialogs and
physical ignition actions remain operator actions. No fixed settling sleeps are
needed in the persistent bridge; every next command is validated by the app.

Successful tool results include `connector_timing` with elapsed milliseconds,
request count and request milliseconds. Metrics exclude arguments, pairing
codes, menu text and vehicle identifiers. These measure the local bridge and
endpoint, excluding the AI/tool-service round trip.

Verification commands:

```sh
python3 scripts/android/test-mcp-bridge.py -v
python3 scripts/android/test-mcp-bridge.py --benchmark-baseline /private/baseline-bridge.py
python3 scripts/android/test-lcd-on-device.py --serial emulator-5560 --suite mcp --expected-version 0.1.1-preview.4 --classes target/android-tech2-release/classes
```

The host protocol tests use synthetic menus and TCP. The Android endpoint suite
uses a synthetic owner and real cache files on an isolated emulator, without
vehicle commands. It covers authentication/revocation, stale state, one-key
admission, lost/no-transition handling, manual takeover, bounded waits,
concurrent health, cache invalidation and fresh command admission. Retain exact
APK, source, signing and benchmark receipts separately. Offline firmware and
physical Fire/adapter checks remain separate qualifications; the old Fire
passes do not qualify this new executable. ARM32 shares source but has no new
package/test claim. Other platform latency bindings remain pending in the
canonical contract and conformance matrix.
