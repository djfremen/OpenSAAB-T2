# Android security access and provider fallback — 2026-09-16

The shared Android workflow always contacts OpenSAAB first. Before processing,
the operator chooses either **OpenSAAB only** or **Allow fallback**. The latter
permits sending the same collected VIN and security data to Bojer at
`https://sas.mysaab.info/api/process` if OpenSAAB is unavailable. This choice
applies to the current attempt, not all future sessions.

## Provider policy

- Primary: `https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/process`.
- Fallback: `https://sas.mysaab.info/api/process`.
- At most one primary request and one fallback request per operator attempt.
- Eligible outages: HTTP 500, 502, 503 or 504; connection timeout, connection
  refusal, DNS lookup failure or no route to host.
- No fallback for HTTP authorization denials, rate limits, redirects, other
  HTTP statuses, TLS/certificate failures, oversized replies, invalid JSON,
  invalid security data or cancellation.
- A primary HTTP 503, including a temporary maintenance response, qualifies as
  unavailable. An intentional authorization/policy denial must use a non-outage
  status such as 403. The app cannot distinguish two different meanings of 503.
- Requests have 15-second connect and 30-second read timeouts; redirects are
  disabled and successful response bodies are limited to 1 MiB.
- Only the existing JSON payload is sent. No OpenSAAB server credential or
  database is packaged or forwarded. Provider credentials must never be shared.
- Bojer availability, service terms and a successful current live security
  transaction have not been established by the synthetic tests.

## Collection, import and continuation

1. The original firmware collects fresh vehicle security data. The app waits
   for the TIS transfer prompt; it does not manufacture a seed or skip key prompts.
2. The emulator session stops. The app checks the guest snapshot's origin,
   write evidence, 714-byte size, fresh unfilled keys and current card baseline.
3. It posts `{"REQUEST_VERSION":1,"SSA_DATA":"<base64 block>"}` to the selected
   provider sequence. A fallback always uses the original collected block.
4. The reply must contain a valid 714-byte `SSA_DATA` block. The VIN, record
   locations, algorithms and seeds must match; keys and the eight-character
   code must be filled. Unrelated byte changes are rejected.
5. The app backs up the entire 32 MiB working card and verifies its hash. It
   patches a temporary copy, verifies all bytes inside and outside the region,
   rechecks the session and original hash, then atomically replaces `card.bin`.
6. **Return to firmware** starts a new normal native session with the updated
   working card and the selected USB adapter. This is a restart, not a jump into
   a suspended CPU state. The operator repeats the intended task and follows
   the original firmware's prompts. Firmware handles the vehicle exchange;
   the vehicle must still accept the returned security data.

The private working card is `<app filesDir>/firmware/card.bin`. Offsets below
describe this established card format, not an arbitrary firmware version.

| Data | Offset |
| --- | --- |
| Complete 714-byte SSA region | Card `0xFE0000` through `0xFE02C9` inclusive |
| VIN, 17 ASCII bytes | SSA `+0x14` |
| Security code, 8 ASCII bytes | SSA `+0x26` (card `0xFE0026`) |
| First seed/key record | SSA `+0x132` |
| Each 8-byte record | 16-bit big-endian status, algorithm, seed, key |
| Returned key within each record | Record `+6` and `+7` |

The whole validated block is written, rather than independently inserting a
code string. This changes emulated card data, not ECU firmware or emulator
instructions. Successful import records `vehicle_access_verified: false`.
It does not prove that every required vehicle module was collected or accepted
access. Switching vehicles requires a fresh collection.

Evidence lives in the private session's `security-api-<UUID>` directory. It
contains raw sensitive security data and the card backup; do not publish it or
attach it to a public issue. `result.json` records the provider actually used,
fallback consent/reason and import hashes. Existing sanitized support reports
remain separate.

## Source map

- `android/shared/com/opensaab/usb/SecurityAccessView.java`: consent UI,
  collection checks, HTTPS transport, reply validation and import orchestration.
- `android/shared/com/opensaab/usb/SecurityApiClient.java`: provider order,
  outage classification and bounded fallback policy.
- `android/shared/com/opensaab/usb/SsaData.java`: SSA layout and validation.
- `android/shared/com/opensaab/usb/SsaCardImport.java`: verified atomic card patch.
- `android/shared/com/opensaab/usb/ChipsoftUsbActivity.java`:
  `openSecuritySession` restarts collection or normal firmware with `card.bin`.
- `android/tests/SecurityApiClientTest.java`: synthetic provider tests.
- `android/tests/SsaDataTest.java`: validation and transactional card tests.

The Java workflow is shared by ARM32 and ARM64 APKs; native binaries, package
IDs and release channels remain separate. No native security handler was added.

## Verification and release status

Local tests passed using `python3 scripts/android/test-request-gate.py`, including
outage/denial/TLS/cancellation cases and existing SSA-only import/backup tests.
All shared Android sources also compiled and passed D8 for minimum API 26.
No live security data was sent and no vehicle was exercised by these tests.

This source change has not been published in a new APK. Before release, exercise
the consent UI on a device, validate the current live provider contract with an
authorized test session, and verify that the original firmware resumes and the
vehicle accepts access. Primary service hardening and deployment are separate
work; client failover does not secure or re-enable that server.
