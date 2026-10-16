# One interface, every platform

The owner's standing product standard is the McDonald's fries analogy: users
should recognize the same OpenSAAB and complete the same tasks on Android ARM32,
Android ARM64, Windows x64, native macOS ARM64 and Intel x86_64, and Linux x86_64.

`interface-contract.json` pins the qualified Android reference source, visual
tokens, responsive layout, labels, key identities and required workflow cases.
The reference source has been checked against the current Android language
checkout; the four pinned interface files are unchanged. Native OS installation,
permissions, file pickers and window chrome may differ. Product controls and
their behavior must follow the contract. Keyboard and mouse operations supplement
touch gestures; they do not replace accessible on-screen controls.

## Implementation rule

One specification governs the platform renderers. A new action or changed flow
must update that specification and the target matrix. Share catalogs,
diagnostic operations and result semantics; keep OS integration and each adapter's
transport implementation behind their existing boundaries. Cosmetic buttons do
not close missing functionality. A platform that lacks a required operation stays
an incomplete preview with an explicit explanation.

The current native Windows and Mac shells approximate the Android reference;
they are not declared exact matches. Linux 0.1.1's installer and offline menus
work, but its old frontend is not the common interface. Port the Linux workspace,
keypad, Actions/app menu, language setup and session states before claiming parity.
Then close the known Windows/Mac differences against this same reference. Preserve
the working Linux package as a rollback checkpoint during migration.

## Evidence required for a uniform release

Use the same firmware version/language, scenario sequence and supported bench
fixture when comparing platforms. Capture the installed artifact at each scenario
listed in the JSON contract. Record OS, app ABI, window dimensions, input method,
adapter identity, firmware identity and observed outcome. The responsive case must
include compact, wide, expanded and restored views. The actions case must account
for every reference menu entry, including unsupported ones.

Each exact-artifact qualification receipt adds:

```json
{
  "interface_conformance": {
    "reference_sha256": "<SHA-256 of interface-contract.json>",
    "scenarios": {
      "<every scenario ID from the contract>": {
        "result": "pass",
        "evidence_path": "<relative screenshot PNG/JPEG/WebP or MP4/WebM>",
        "evidence_sha256": "<exact evidence hash>"
      }
    }
  }
}
```

A continuous walkthrough may support several scenarios; the reviewer must
document where each occurs. Missing scenarios, a different reference hash, failed
outcomes, altered evidence or non-media files block uniform-release qualification.
The gate validates integrity and coverage, not the truth of visual or hardware
claims. A human or UI-observing reviewer must compare actual behavior.

Existing packages and historical evidence retain their original scope. The new
contract is not a claim that any existing artifact already consumes it or passes
all scenarios. The matrix deliberately marks this new requirement incomplete.

## Branding and voluntary support

The `branding` scenario also requires the approved logo in the application
header and a visible Support OpenSAAB callout. Preserve the source logo's aspect
ratio. Use the Android support copy and confirmed Ko-fi destination in the JSON
contract; show the voluntary-support explanation and an explicit Continue action.
Opening project support must not interrupt an active session. Both compact and
wide views must keep the logo, callout and firmware controls reachable.

## Standard setup profile

The Android reference packages the approved `eprom.bin`, `opsys.dwn` and
`candi.bin` support components and downloads the chosen Saab diagnostic image
separately. `support-files.json` pins that exact Android profile for desktop
packaging. A normal installation must not send the user looking for a Windows
runtime folder before reaching the original firmware menus. First launch should
present Diagnostic language, Software version, Download and continue, then return
to Connect and start / Run without an adapter. Valid existing components, card
selection and logs must survive app upgrades. Explicit import-only packages are
separate profiles and must not be presented as the standard setup path.

Support-file source licensing is unchanged. Packaging metadata must distinguish
bundled support components from a diagnostic card. Setup equivalence does not
establish DTC, security, adapter recovery or live-emulator feature equivalence.

## Security data and freshness

The shared `security_status` scenario preserves Android's NoMoreGlobal labels:
`[INIT_AUTH]`, `[PRE-AUTH]`, `[POST-AUTH]`, `[INVALID]`, `[N/A]`, plus Fresh,
Stale and Age unknown. Stale starts at three hours from the recorded card import;
it is advisory data age, not an ECU authorization expiry. Match both vehicle and
SSA hashes before using a receipt. Reopening must never refresh timestamps.

Desktop implementations prefer the guest working SSA checkpoint over the startup
card copy, refresh while idle/running, and
provide details through Actions even with the header hidden. The status renderer
must never write the card, invoke a security service, or send an adapter command.
Processing/import/reset remain separate capabilities; displaying these labels
does not qualify those operations. The synthetic corpus in
`emulator/tests/security-status/` is shared by Python, Swift and C#. Run Android's
reference security-state/history tests as well. Installed UI evidence and real
processing qualification are separate from these deterministic tests.

## Changing diagnostic software during emulation

Firmware selection remains reachable after VIN discovery starts the emulator.
Offer a clear stop-and-select action and a keep-running option. Wait for the
session to exit and release its adapter before changing the selected image;
cancelling leaves both session and image untouched. After selecting software,
return to Ready and require Connect and start for a fresh vehicle connection.
Qualify this transition separately on each platform; shared labels alone do not
prove teardown or selection preservation.

## Collected security data persistence

Guest flash changes must reach the displayed working state. Mac 0.2.13 publishes
an atomic 714-byte session checkpoint when flash returns to read-array mode.
Legacy final guest snapshots can be read with their matching metadata. Session
creation restores only recognized PRE/POST data matching the current vehicle and
firmware into a new disposable card. Immutable downloads and previous evidence
remain unchanged; receipt timestamps are copied verbatim. The status renderer
performs no writes. This fixes state visibility/persistence, not processing or
ECU authorization. Other targets require the same live collection/reopen check.

## Chipsoft session modes

Mac 0.2.14 binds the Android session-mode selection to the existing shared Rust
backend. Full native control follows original guest commands while retaining frame,
electrical-route and wire validation. Read-only diagnostics remains selectable and
remembered; seed collection stays a separate session. CDC owns the device instead
of the Android USB service but no longer forces a different command policy.
On worker termination the shell clears the stopped framebuffer and live connection
claim, retains logs and provides reconnection guidance. This is a scoped integration;
it does not qualify all commands or finish security processing/report workflows.

## Security processing workflow

The shared `opensaab-security` worker ports Android's validation, provider decision,
recognized-menu navigation and SSA-only import rules. The macOS binding owns
consent/password dialogs, Keychain, bounded HTTPS and emulator lifecycle. Collection
requires fresh flash evidence and the completed transfer prompt. The original
card download and collected evidence are retained; a verified reply activates a
new working session atomically. POST-AUTH/Fresh is imported-data status, never an
ECU authorization claim. Resume checks the original CDC port and GET_INFO reply;
this is not a fresh VIN read or proof that the vehicle stayed connected.

No physical key prompt is acknowledged automatically. An operator key cancels
menu assistance. Primary and fallback are separate disclosure decisions; no
credentials reach fallback, no redirects are followed, and invalid successful
responses never trigger fallback. The platform credential store is opt-in.
Windows/Linux bindings and Android shared-core adoption remain pending. Each
installed artifact needs its own end-to-end and live authorization evidence.

## Reusing collected security data

Mac 0.2.16 separates Process existing data from Collect fresh data. Reopened or
offline sessions can process their selected PRE-AUTH without another VIN read.
The shared worker verifies the exact selected seed bytes against completed guest
collection evidence and unchanged firmware outside SSA. A matching VIN alone
is insufficient. Legacy sessions use bounded evidence discovery; new restores
preserve collection lineage. Selection changes are rejected before preparation
and import. Existing POST-AUTH offers continuation rather than recollection.
The user chooses when to replace saved seeds with a fresh collection.

## Passive saved DTC reports

Mac 0.2.17 replaces the Saved DTC reports placeholder using the shared Rust
report worker ported from Android DtcReport/View. Stable numbered firmware
screens are transcribed, grouped and saved locally with explicit partial-list
coverage. Opening a detail screen can add the full description. Original screen
observations remain in JSON. A session-local VIN snapshot prevents offline or
unrelated-session identity leakage. Review, export and OS sharing are native
bindings; capture sends no firmware keys, CAN messages or network requests.
The two Mac architectures build these exact same sources. Their installed/live
qualification remains separate; Windows/Linux bindings remain pending.

## Trionic 8 startup code report

The desktop reader ports the Android `chipsoft_dtc` exchange: fresh VIN, diagnostic
session, current/history A9/81 report, verified end marker, session stop and channel
release. macOS 0.2.20 adds native review/save/Continue around this shared operation.
The original-menu path remains available for other ECUs. Do not infer ME9.2 or
all-module support. Report completeness is separate from vehicle-wide coverage.
The two owner ECM fixtures share a VIN; qualification distinguishes bench and car
by physical fixture, never by VIN alone. Windows standalone comparison, installed
Mac workflow and full-car handover each need their own evidence.

## Adapter failure presentation (scoped desktop preview)

Windows 0.2.11 and macOS 0.2.21 consume the same Rust `adapter_failure` object
from the completed session report. The classification preserves Android's rule
that Chipsoft status 001D alone does not identify a single-wire wake failure; the
last submitted frame must also match that wake request. The OEM comparison
transport can report the corresponding J2534 write timeout. Unknown failures
remain unknown; no reply is fabricated and no failed command is retried.

A stopped session clears its old framebuffer, explains the failure and retains
its logs. Support reports include only an allowlisted failure kind, not the raw
adapter error. Windows runtime readers use delete sharing so reading a screen
snapshot cannot block its atomic replacement by the emulator. These changes do
not implement Windows report upload, full session-mode parity or security
processing. Exact-artifact evidence and hardware limits are recorded in
`WINDOWS_0211_RECEIPT_2026-09-24.json`.
