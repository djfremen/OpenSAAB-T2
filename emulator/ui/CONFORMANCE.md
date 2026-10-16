# One interface, every platform

## Developer Preview welcome — October 14, 2026

Every fresh app launch presents Developer Preview, the full packaged version and
architecture, and the packaged release date before setup, workspace or diagnostics.
The date line says Version date: using the packaged date for both testing and
distributed APKs. Developer Preview is the channel shown to users; private/public
packaging bookkeeping does not appear on the splash. Public packaging must supply
an explicit version date; never use installation or launch time. Missing or
inconsistent metadata stays unavailable.

Continue opens the app. Report issue opens the existing report preparation/review
flow without automatic preparation, consent or upload. Returning to the welcome,
rotation and resuming an existing diagnostic session must preserve their state;
only a new fresh app launch repeats the notice. Use the shared logo, colors and
accessible buttons, keep the support footer, and allow compact/large-text scrolling.

Android ARM64 preview.85 is the next planned package after the current debug
session. Its packaging and installed checks remain pending. Public-facing ARM64
development versions retain preview.N; findings remains internal research/history.
The current findings.13 debug installation and its scoped evidence are preserved.
Chipsoft, MDI and Nano tests follow the debug phase using the Android implementation
and MCP controls. Each other target's native binding/package metadata and installed
verification remain separate in the baseline matrix.

October 12, 2026 owner requirement: every new development attempt includes its
full version, experiment/revision suffix and architecture in the installed
program name. `interface-contract.json` records this as `program_identity`.
The canonical Mac builder now uses a versioned `.app` name, matching bundle
display/program names, disk volume and manifest; the main window and About/Quit
menu entries use the packaged version. `--version` selects a distinct attempt
without leaving a stale compiled display version. The new default is
`0.2.24-mxp-core-preview.2`; this is a source change for future packaging, not a
new installed or published artifact. Verify Finder/Dock/window identities and
metadata together on each exact bundle before deployment. Mac ARM64/Intel
package and installed checks are pending; Android ARM64/ARM32, iOS, Windows and
Linux bindings remain pending adoption. Historical packages retain their exact
names and qualification scope.

October 4 Windows Bluetooth checkpoint: 0.2.17 adds native WinRT GATT and
paired RFCOMM effects around the exact shared Rust 0.1.3 startup/protocol/report
worker. After the user rebooted the Windows 11 ARM Fusion guest, the exact x64
launcher and packaged worker passed 67 actual Framework binding checks and the
saved shell checks. All 22 private candidate hashes and all 21 public portable
payload hashes match. The public package opens with the existing firmware cache,
shows the Bluetooth/VIN/ECM/code tools, clears current identity, and handles the
native API's no-radio result. A separate dev-preview desktop shortcut is present.
Physical BLE/RFCOMM reads, native Windows x64, full common visual/workflow coverage,
MSI install/upgrade and clean first-time firmware download remain pending.
Fusion 13.6 removed host Bluetooth sharing; the actual native API reports no guest
radio. Existing Apple artifact receipts retain their packaged contract hashes.
Runtime Apple labels and protocol sources are unchanged by this extension.

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

## Discord community contact (October 7, 2026)

About / Support includes the shared `community_label`, `community_message` and
`community_url` from `branding.support`. The approved permanent invite is
`https://discord.gg/FxnFe8vQht`. Open it only after the user selects Discord
community; append no vehicle data, logs or contact identifiers. Preserve the
existing active-session guard and donation action. Collector exposes the same
contact as a separate action and prevents navigation during capture/upload.

Source adoption does not update an already published installer. Track the next
build and its compact/wide support dialog, explicit browser launch, cancellation
and session guard separately for each target. Historical artifact receipts retain
their original contract hash and scope.

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

October 14, 2026 owner requirement: every version and platform requires the
case-sensitive password `OpenSAAB` before security-access processing or importing
a processed response. Apply the gate to UI, automation, remembered credentials
and fallback paths. Blank, wrong, case-changed or cancelled credentials must not
start processing/import. A target without processing support keeps it unavailable.
Verify each installed package and retain its hash with positive and negative
results; previews cannot waive this release check. Correct password acceptance
does not prove a vehicle grant. Raw seed collection retains its separate existing
workflow and does not itself process or grant security access. The shared
contract records the policy; installed verification is pending on every target.

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

## iOS offline milestone (October 1, 2026)

The initial arm64 SwiftUI app is an explicitly incomplete, import-only profile,
requested by the owner so that neither Vetronix support files nor a diagnostic
image are bundled. It consumes the same interface contract, key encoders, approved
logo and voluntary-support text as Android. Native Files import validates the
selected support component before replacement. This profile is separate from
standard automatic setup; downloads and coordinated language selection remain
pending.

The in-process Rust boundary uses the shared interpreter, guest step routine,
bus/LCD and existing research-harness/deferred-CANdi profile. The simulator
walkthrough is evidence for offline original menus only. It is not physical Tech 2
POST fidelity, Android parity, a live adapter result or vehicle authorization.
The app displays Offline and auth_status [N/A]. Guest card changes are ephemeral
in this milestone; no security service or transport is opened. Firmware images
used in tests remain outside app/test resources and source control.

See `ios/README.md` and the scoped milestone receipt in
`releases/consistent-baseline/IOS_OFFLINE_MILESTONE_2026-10-01.json`. Bluetooth LE
through vLinker MC+, VIN/DTC workflows, security processing/persistence, reporting,
updates, crash investigation and physical-iPhone qualification remain migrations.

## Reporting/session contract adoption, 1 October 2026

The approved reporting/session rules are now normative in `report_review`,
`session_end` and `diagnostic_prerequisites`; the Android first pass adopts only
the independent stopped-session dialog, rotation-only consent, exact artifact
restoration/receipt, end presentation and Actions fixes. Its Java collector remains
a transition. Active-session reporting, the Rust collector/formatter port and
server deduplication remain separate gates. The historical pinned Android files
remain the visual reference; they do not certify these newer candidate changes.

New scenario adoption is recorded per target in `report_review_first_pass` in the
baseline status. Private iOS build8 and old desktop receipts retain their exact
old artifact/contract scope; synchronizing contract source is not rebuilding or
qualifying those apps. No uniform release is declared.

## Saved firmware swipe preference — October 1

App menu → Preferences offers Natural scrolling for vertical gestures on the
original firmware display. Default off preserves Android's current up-to-UP and
down-to-DOWN behavior. Enabled maps up-to-DOWN and down-to-UP, using the same
encoder identities; one deliberate swipe sends one key. The common JSON supplies
both mappings and the explanatory labels. Native preferences persist the choice
per device and apply it immediately. Do not invert keypad/hardware arrows,
hold-to-ENTER, plain taps or scrolling in the app's own lists.

The `gesture_scrolling` scenario requires original-menu evidence for both modes,
relaunch persistence, unchanged controls and no firmware key or session restart
when changing the preference. iOS build 10 implements the native preference.
Android source also implements it at local commit `105d7a2` on
`codex/android-natural-scrolling`; compilation and the common-contract binding
check pass. After initially deferring deployment, the owner authorized private
ARM64 preview.30 installation on Pixel 7 for manual testing. In-place installation,
version read-back and launch succeeded; ARM32 and public releases are unchanged.
Both Android ABIs still need original-menu, relaunch and upgrade checks before qualification;
see `OpenSAAB-T2:docs/android/NATURAL_SCROLLING_2026-10-01.md`. Desktop adoption
and each installed target's qualification remain tracked separately in the
status matrix. Existing published artifacts are unchanged.

## vLinker simulator bench workflow — October 2

The common `vlinker_bench_test` metadata preserves Android bench labels and
scoped result semantics. iOS build 11 adds a Debug simulator binding through a
local Mac BLE relay. Shared Rust owns the command sequence, framing, deadlines,
VIN validation and cancellation; Swift supplies transport and native display.
This host bench result remains separate from an offline firmware session.
Build 12 fixes the missing Debug compilation condition. The installed iOS27
simulator app on the M4 displays the fresh bench VIN after one request, VTPC OK
and confirmed BLE release; the live UI test and three exact-build native checks
pass. The earlier build-11 offline menu regression retains its original scope.
Direct physical-iPhone BLE and full firmware diagnostics remain missing. Other
targets have not adopted or qualified the shared replacement; Android retains
its working Java reference. See the iOS repository receipt
`releases/consistent-baseline/IOS_VLINKER_SIMULATOR_LIVE_2026-10-02.json`.

Build 13 restores the adapter page’s shared OpenSAAB brand header and palette,
with raw transport details under Console. Its installed simulator UI open/close
check and screenshots pass without a new vehicle request. The build-12 live VIN
receipt retains its exact scope; this is not normal Tech 2 connection integration.

## Chipsoft-style VIN-first startup — October 2

The shared `connection_startup` coordinator and `adapter_startup` labels port the
Android Chipsoft launcher/discovery/VehicleSession sequence: firmware readiness,
selected adapter, fresh VIN, confirmed discovery release, bounded matching vehicle
lookup and durable per-attempt identity. Adapter commands remain separate; vLinker
uses its own shared protocol. Cancellation retains ownership during transport
cleanup, clears pending identity and rejects late callbacks; history never creates
a current connection. Typed support records contain no VIN or raw payloads.

Private iOS build 15 follows this path from the normal Connect and start button.
Seven installed iOS27 simulator native/UI checks pass, including one live VIN and
matching descriptive vehicle lookup, picker cancel, original offline Main Menu,
and clearing identity on offline start and cold relaunch. Screenshots and exact
installed payload hashes are in the iOS startup receipt. Shared startup/protocol/
report fixtures and the preserved Chipsoft backend tests pass.

The vLinker firmware/CANdi transport is unavailable, so startup ends with an
explicit firmware-unavailable result after identification. It does not start an
offline engine as a connected session. Direct iPhone BLE, Android instrumented
equivalence/shared-core cutover and other target bindings remain incomplete.
Physical iPhone build 10 and Android installed artifacts are unchanged. Receipt:
`OpenSAAB-iOS:releases/consistent-baseline/IOS_VLINKER_STARTUP_2026-10-02.json`.

## Android/iOS aligned VIN startup — October 2, 2026

Exact installed Android ARM64 preview.34 (100034), Android16/API36, and iOS
ARM64 build 18, iOS27, read the same fresh real VIN through the local M4 BLE
helper. Both retain the shared Rust startup/protocol decisions and display the
same 2004 9-3 / 9440, B207R, color, gearbox and body descriptions. The normal
workspace follows Android's compact tappable summary, stacked Connect/Offline
buttons and App menu firmware selection. Vehicle details show observation time;
saved security history remains separate from current identity and ECU access.

Eight exact-build iOS native/UI checks, two Android native suites, Gradle package
verification/lint and six shared startup plus six shared protocol tests passed.
Each final live test wrote one VIN request, completed VTPC and confirmed BLE
disconnect. Picker cancellation, original offline menus and clearing current
identity were checked; iOS cold relaunch does not inherit a live VIN. Actual main
and details screenshots were compared, preserving their pixels apart from
uniform resizing and external labels. Native font/icon/sheet differences remain.

Receipt: `releases/consistent-baseline/ANDROID_IOS_VIN_ALIGNMENT_2026-10-02.json`
in canonical OpenSAAB and iOS, also copied to Android's `docs/android`. Raw VIN,
reports, traces and screenshots remain in the private ignored bench audit path
recorded by the receipt. Full vLinker firmware/CANdi diagnostics, direct physical
phone Bluetooth and other target adoption remain unqualified. Physical iPhone
build 10 and Android phone installations were not changed. This scoped preview
does not close the existing uniform-release inventory schema/evidence gate.

## Trionic 8 HS-CAN ECM probes — October 2, 2026

Exact installed Android ARM64 preview.36 (100036), Android16/API36, and iOS
ARM64 build 20, iOS27, pass the same explicit ECM identification and current/
history DTC probes on the M4/vLinker bench. Each probe starts with one fresh
VIN. All seven identifier values and raw bytes match; the complete six-code
report, failure types and status bytes match. The raw 5E8 end marker, positive
diagnostic stop, VTPC and BLE disconnect are confirmed before results appear.
No codes were cleared and no programming/security command was sent.

App menu → HS-CAN ECM probe exposes the same two read controls and Stop test.
Shared Rust owns command selection, decoding, bounds and result text; native
Java/Swift perform UI and loopback Bluetooth effects. Actual side-by-side
ECM-information and fault-report screenshots preserve the captures apart from
uniform resizing and external labels. Native fonts/headers/sheets still differ.

Eleven exact-build iOS native/UI checks and five Android native suites passed,
including the normal VIN startup/offline regression, original Main/Chipsoft
layouts and probe controls. Eleven shared protocol and six shared startup tests
passed. Exact package verification, Android lint and installed payload hashes
passed. The earlier iOS19 runs stopped at renewed macOS Bluetooth consent and
sent no vehicle commands; final iOS20 and Android36 live tests pass.

Receipt: `releases/consistent-baseline/ANDROID_IOS_HSCAN_PROBES_2026-10-02.json`
in canonical OpenSAAB/iOS and Android's `docs/android`. Raw VIN, reports, traces
and screenshots remain in the private ignored bench audit directory named by
the receipt. The downloaded Trionic source is pinned by hash, as its archive
has no Git metadata. This qualifies an ECM-only T8 mask12 current/history
report, not every module/status or a physical CEL diagnosis. Direct phone BLE,
other target adoption, full original-firmware vLinker/CANdi diagnostics and the
existing uniform-release inventory schema/evidence gate remain pending.

## Offline report evidence candidate October 2 2026

Private Android preview.37 adds stable offline session correlation, input fingerprints,
fixed native/CANdi stop reasons, host stop intent, bounded first/last resource samples
and explicit notes presence. API31 emulator telemetry and reporting regressions
are checked; physical Android and all other installed targets remain unqualified.
Desktop/iOS collector forwarding is unfinished. The expanded schema is tested
locally but has not been deployed. See `OFFLINE_REPORT_EVIDENCE_2026-10-02.md`
in `releases/consistent-baseline` and its exact-artifact JSON receipt.

## vLinker stability checkpoint October 3 2026

Android ARM64 preview.37 (100037) and iOS ARM64 simulator build23 pass the
shared VIN-first startup and HS-CAN ECM information/current-history probes
with matching identity, seven identifiers and six raw code records. A single
pinned opensaab-vlinker-workflow 0.1.0 dependency now owns these workflows,
connection leases and complete saved reports. Timeout cleanup, cancellation,
truncated responses, disconnect, overlap, stale callbacks and fresh reconnect
are covered by native/shared fixtures. Both apps reopen saved observations
after restart and expose explicit export without establishing live identity.

Eleven distinct iOS native/UI checks, eight Android qualification runs and
24 shared tests pass, as do Android build/lint, firmware-free package checks,
exact installed-payload checks and the modified-source rejection test. Actual
side-by-side captures were inspected; native typography/sheets still differ.
Private source/artifact/rollback archives and their hashes are frozen. The
generated qualification ledger follows the source snapshot, avoiding a
self-referential archive hash.

Direct iPhone CoreBluetooth is implemented and compiles for ARM64 iOS; it has
not been installed or tested physically because the owner's device is
unavailable. The iPhone remains build10. Full original-firmware wireless
transport, SW-CAN, other targets and uniform-release qualification remain
pending. The broader emulator core still has separate checkouts. Crash-report
interpretation and broader error handling remain in the owner's other thread.

Receipt: `releases/consistent-baseline/ANDROID_IOS_VLINKER_STABILITY_2026-10-03.json`.
Details: `docs/VLINKER_STABILITY_CHECKPOINT_2026-10-03.md` (Android: `docs/android`).
Raw VIN, CAN data, reports and screenshots remain in the private ignored audit.

## Direct Android Bluetooth implementation October 3 2026

Private Android ARM64 0.1.0-preview.38 implements direct Classic SPP with native
permissions, paired/nearby selection, bounded I/O, Stop and shared VIN/ECM effects.
Thirteen production-stream endpoint fixture cases, 24 workflow0.1.1 tests, five
Debug regressions and release permission/selection UI checks pass on API36. The
signed phone APK and installed release counterpart have identical unsigned payloads.
No physical Pixel7 install/read occurred. Real radio/discovery/read/recovery and
ARM32 packaging remain pending. Installed iOS23/iPhone10 and older live parity
receipts retain their original workflow0.1.0 scope. Other targets inherit no pass.

Receipt: `releases/consistent-baseline/ANDROID_DIRECT_VLINKER_BLUETOOTH_2026-10-03.json`.
Details: `docs/VLINKER_ANDROID_DIRECT_BLUETOOTH_2026-10-03.md` (Android: `docs/android`).

## Bluetooth adapter model identification

Android uses the common adapter selector for Carista EVO and vLinker MC+. A
Bluetooth name locates a candidate; shared VTI/VTVERS validation selects the
model and its command profile. The SPP UUID and MAC do not establish the model
or CAN capabilities. After a fresh VIN read, show the actual adapter model and
measured voltage alongside the VIN, then describe the released connection.
EVO VIN qualification does not qualify ECM probes, SW-CAN or original-firmware
wireless transport. Other installed platforms retain their existing MC-only
behavior until their picker, transport profile and hardware reads are verified.

The preview.43 Pixel 7 receipt separately qualifies EVO MIC3413 V2.0.84 host
Trionic 8 HS-CAN reads: seven identifiers and a complete current/history mask12
code report, each with a fresh VIN and confirmed native release. DTC success
requires an explicit end marker and positive diagnostic stop. The existing
HS-CAN ECM probe and saved-report workflow is shared between MC+ and EVO;
results preserve the actual model and operation profile. Other EVO firmware,
SW-CAN and original-firmware wireless operation retain their qualification gaps.
Receipt: `releases/consistent-baseline/CARISTA_EVO_PIXEL7_ECM_2026-10-03.json`.


## EVO VIN simulator comparison October 3 2026

Installed Android ARM64 preview.44 (100044), Android16/API36, and iOS ARM64
build26, iOS27, pass the normal Connect workflow through real Carista EVO on
the local M4. Both show the same fresh VIN, actual adapter model and 12.0V,
with matching descriptive lookup. Each writes one VIN request, receives VTPC OK
and confirms native Bluetooth release before publishing success. Offline clears
current identity; the iOS live test also checks original Main Menu and cold
relaunch clearing. Five iOS native checks and seventeen Android synthetic
endpoint cases passed, and installed payloads match the tested builds.

The explicit loopback-only helper uses macOS's observed SPP record/channel and
IOBluetoothRFCOMMChannel. It transports bytes and confirms native writes; shared
opensaab-vlinker-workflow 0.1.3 owns protocol selection and VIN decoding. Actual
VIN/details/offline screenshots are preserved privately and displayed side by
side with proportional CSS scaling. Native font/status/footer differences remain.

Receipt: `releases/consistent-baseline/ANDROID_IOS_EVO_VIN_2026-10-03.json` in
canonical and iOS, also in Android `docs/android`. The receipt distinguishes
the tested embedded interface hash from later documentation-only specification
updates. Physical Pixel7 preview43 retains its separate receipt. Physical iPhone
EVO BLE, I-bus/Add/Remove ECM, full original-firmware wireless transport, other
architectures and uniform-release qualification remain pending.


## Apple Bluetooth effects — October 4, 2026

The common Apple BLE source adds physical iPhone Carista EVO discovery alongside
vLinker MC-IOS and requires explicit device selection. EVO's observed M4 GATT
profile is the same 18F0 / TX 2AF1 / RX 2AF0 profile used by MC-IOS. Shared Rust
VTI / VTVERS validation identifies each product and gates its firmware profile.
UUIDs identify transports; they do not identify adapter models.

macOS 0.2.22 ARM64 and x86_64 packages include the pinned 0.1.3 connection worker,
shared Apple BLE source, native paired Classic SPP, a device picker and bounded
VIN / Trionic 8 HS-CAN ECM reads with saved reports. Bluetooth permission is
requested before paired enumeration; enumeration runs off the AppKit thread.
The original first-launch main-thread stall was reproduced and fixed. The final
Mac window remains responsive and Cancel returns to an idle, unidentified state.
Native and shared ownership remain held until selected transport teardown.

ARM64 native endpoint fixtures cover both products over BLE / Classic, one fresh
VIN, seven ECM identifiers, complete mask12 code reports, saved-report reopen,
wrong profile / firmware, overlap, cancel, disconnect and unconfirmed release.
The installed ARM64 app also performs actual M4/EVO reads over both BLE and SPP.
Read scope and exact hashes are recorded in
`releases/consistent-baseline/APPLE_BLUETOOTH_2026-10-04.json`.

The Intel package is a native x86_64 cross-build with payload / framework / strict
ad-hoc signature verification. This M4 has no Intel execution runtime; neither
Rosetta execution nor physical Intel Bluetooth has been qualified. Physical
vLinker on the new Mac package is unqualified. iOS 0.1.1 build27 compiles for
physical ARM64 and has selected simulator regressions; development signing is
pending Keychain access and physical iPhone qualification requires the phone.
The standalone Mac app's first Bluetooth grant needs a human because the system
permission dialog is outside the automation tool's allowed controls. Native
product CLI hardware tests use the caller's existing OS Bluetooth grant and are
reported separately from standalone GUI qualification.

VIN / host ECM support does not implement SW-CAN, programming, security or
original-firmware wireless/CANdi transport. No full product, App Store, TestFlight,
notarization or uniform release qualification is claimed by this checkpoint.

## Published Bluetooth development previews — October 4, 2026

Android ARM64 preview.45, macOS0.2.22 ARM64/Intel and Windows0.2.17 portable
previews are public, with matching source, notices, checksums and 19 independently
verified GitHub asset downloads across these releases and the private iOS build27
source preview. The live website/catalog selects these packages; ARM32 remains
headunit.17. The exact signed Android APK upgraded/cold-launched on Pixel7 and its
installed APK/ARM64 worker pins match; fresh release.45 vehicle reads remain pending.

The public Mac packages were rebuilt from clean canonical1ccdd149 with local Rust
paths remapped, and are distinct from the earlier staged0.2.22 binaries. Both
architectures passed payload/signature/privacy checks. The exact M4 package passed
120 checks and six fresh EVO CLI VIN/ECM-information/complete six-code reads over
BLE and Classic, with native closure, plus four saved-report reopens. Intel execution,
physical vLinker, fresh standalone GUI VIN and older macOS remain pending.

The public Windows ZIP's21 payloads match the tested Fusion guest;67 actual
Framework binding checks, saved shell checks and visible no-radio cleanup passed.
That Windows11 ARM guest has no Bluetooth radio. Native Windows x64, hardware reads,
first-time firmware download and MSI lifecycle remain pending. iOS27 source and
unsigned ARM64 compilation are available; no installable IPA or phone BLE pass.
These are read-only VIN/host Trionic8 HS-CAN previews. SW-CAN/I-bus, Add/Remove ECM,
clearing/programming, original-firmware wireless transport and uniform release
qualification remain incomplete. Historical milestone receipts retain their scope.

Receipt: `releases/consistent-baseline/BLUETOOTH_DEV_PREVIEWS_PUBLICATION_2026-10-04.json`.

## Windows Nano driver preview — October 4, 2026

Private 0.2.18 preparation retains the common Connect and start workflow and
adds an explicitly selected OpenVCX32 Nano provider alongside the existing
VXDIAG OEM comparison. One Windows loader binding supplies the selected
absolute DLL and checksum to discovery, VIN and bridge helpers. Missing or
failed providers never cause an automatic switch. Existing Chipsoft and
Bluetooth routes remain separate.

The OpenVCX route is scoped to fresh HS-CAN VIN identification with confirmed
driver cleanup. Its normal driver rejects the Saab pin-1 route used by our
two-channel firmware bridge. After successful identification, show firmware
transport unavailable before bridge startup; offline emulation cannot stand in
for a connected session. The OEM firmware route retains its prior limitations
and requires explicit selection.

The private native M4 software-restart result does not qualify this Windows
app, its driver variant, physical power-loss startup or live-car/SW-CAN use.
Exact source, builds, installed UI and hardware outcomes belong to separate
Windows receipts. See `emulator/docs/adapters/NANO_WINDOWS_ELITEBOOK_FINDINGS_2026-10-04.md`.


## Android Nano security and firmware responsiveness — October 5, 2026

Owner-operated private Android ARM64 nano-init.4 on Pixel 7 / GM Nano 1.9.4.2
completed original Get Security Access collection (12 records / 714 bytes),
OpenSAAB processing and SSA-only import. The original 2004 Saab 9-3 Sport
Add / Remove → SPA → Add workflow reported Checking Security Access OK,
Adding Unit OK and Add / Remove ECU finished. Those original results establish
the observed vehicle operation; the API receipt remains import-only and does
not itself claim a vehicle access grant. The passenger-window antipinch notice
was retained without a separate learning procedure. Final individual bus-config
or DTC-clear OK rows were not retained. After completion, normal Stop received
six shutdown replies 00 and confirmed native/Java USB closure and QUIT. The
manual harness retained incomplete/exit 3; that status is distinct from the
original completed Add task.

Private nano-init.5 reuses the existing shared NativeLcdPump and
InteractiveKeyPump in the Nano firmware screen, with background image decode,
event-driven display updates and queued keypad mailbox writes. Actual Pixel 7
menu tests measured nine keys at 0–2 ms from queue to mailbox and 19 distinct
guest frames at 5–68 ms from file publication to ImageView assignment (median
11 ms). Repeated unchanged images are identified separately in the receipt;
these are not touch-to-ECU measurements. Stop closed both channels/USB cleanly,
with zero diagnostic transmissions in this menu-only test. The successful Add
procedure was performed on init.4 and was not repeated on init.5.

The UI/key semantics follow the existing common interface contract. This
checkpoint qualifies these private ARM64 artifacts on one Nano and vehicle;
ARM32, other adapters, Windows/macOS/Linux/iOS, physical removal of both USB
and OBD power and uniform/public-release qualification remain pending.
Published Android preview.45 is unchanged. Preserve the software-reboot VIN,
Service key-status and partial DTC checkpoint scope separately.

Receipt: releases/consistent-baseline/ANDROID_NANO_SECURITY_SPA_2026-10-05.json.
Matching source, private artifacts and raw logs are pinned there; VINs,
seeds/keys, firmware and API payloads remain private. The Facebook milestone
was published to Opensaab T2 and its permalink is included in the receipt.


## Public Android Nano beta preview.56 — October 5, 2026

Published signed, nondebug Android ARM64 0.1.0-preview.56 (code100056), source
e849c67187d035639e31cc354b979fb5634c1445, APK SHA-256
f96f6300c170b5a00a6efb5060c7a04ccb97a52aad1549c8401629d423f1a81d.
All five public release assets were downloaded and verified byte-identical.
The exact APK upgraded Pixel7 with retained firmware/security data, cold-launched,
established a fresh Nano session with two DH exchanges, accepted both channel
setups, displayed original Main Menu/Model Year and handled five keypad inputs.
Menu-only Stop received all six shutdown replies and closed native/Java USB with
QUIT, zero diagnostic transmissions and no native worker remaining. Device is
now stopped. Fresh vehicle VIN/security/SPA operations were not repeated on this
exact package; the earlier private successful procedure remains a separate receipt.
Full removal of USB and OBD power, other hardware and uniform qualification remain
pending. This follow-up supersedes the earlier note that public preview.45 was unchanged.

Release: https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.56
Website download/updater deployed and verified; ARM32 release unchanged.
The complete SAAB Central BBCode draft is now live at https://post.opensaab.com/,
revised 5 October, with the existing attachment references. HTTP/HTTPS download
and embedded text match; Copy full BBCode displays its success confirmation.
No external SAAB Central post was edited. Explicit credit to Erik Fuller (erik683)
and OpenVCX identifies the initialization code adapted into our Rust backend.
Original copyright/licence assets and matching rebuild source accompany the release.

Receipt: ANDROID_NANO_PUBLIC_BETA56_2026-10-05.json. Historical private receipts,
performance measurements and the earlier Facebook milestone remain unchanged.

## Android report/security workflow fix — October 5, 2026

Private Pixel7 mdi-main4 closes a newly confirmed, durably recorded report upload, retains failed reviews, displays matching working PRE-AUTH ahead of startup-card history, and offers saved processing before MDI discovery. Stale data age never gates fresh collection. Eight isolated Android suites and native atomic checkpoint tests pass. Exact installed saved-seed and metadata preservation is verified; no production upload, security API disclosure or fresh ECU authorization was performed. See `emulator/docs/android/REPORT_SECURITY_WORKFLOW_FIX_2026-10-05.{md,json}`. Other targets, fresh MDI traffic and complete common qualification remain pending.

## Android MDI owner SPA/key-status proof — October 5, 2026

The exact private Pixel7 main4 APK now has owner-operated live original-firmware proof: fresh native collection/API import, SPA security check OK through `ECU finished`, and ignition key status Working (Added), four programmed CIM keys,433MHz. The console shows all eight DTC list positions; the saved exporter captured only four. The session subsequently ended at the structured-capture size boundary with clean native/Java USB release and no pending TX. UI ending state and host authorization receipts did not recognize the manual completion and remain tracked gaps. A separate Remove/CEL action, decoded ECU grant, cold/power-loss and full/common/public qualification remain pending. Collection was passive; no firmware keys or Stop were sent. See `emulator/docs/android/MDI_OWNER_SPA_KEY_STATUS_PROOF_2026-10-05.json`; the earlier workflow regression scope remains unchanged.


## Classic MDI identity during initialization — October 5, 2026

Android ARM64 preview.63 first obtains a fresh management serial from the selected
USB carrier before management login or a vehicle request. Saved and USB descriptor
serials are not handshake inputs. With no base-key profile, identify the adapter,
release USB, then prompt to import the connection key. Firmware setup follows
identity as well. Retrieval stays bounded and cancellable. Schema2 has no serial
field; legacy profiles retain key compatibility with their serial ignored.
Pixel7 private key-only and no-profile checks are scoped separately from the exact
public APK receipt. ARM32, Windows/macOS/Linux/iOS MDI workflow parity remains
pending; the common target matrix in interface-contract.json records those gaps.


## Huawei MDI USB configuration candidate — October 5, 2026

Huawei EVR-L29 Android10 preview.63 retained nine failures after USB open and before configuration selection. The owner photo confirms private `0.1.0-mdi-huawei.1` also fails: “MDI configuration selection failed · USB closed.” Android10 Java release reconnects a kernel driver, undermining the temporary detachment in that patch. Corrected `0.1.0-mdi-huawei.2` uses native temporary release without reconnect, retries only EBUSY and exposes actual errno. Host regression checks, 17 native tests, strict checks, ioctl ABI assertions, API28 ARM64 emulator JNI compatibility and actual Huawei API29 native load/JNI calls pass. The Huawei check uses invalid descriptors, opens no USB device and qualifies no handshake. Its temporary self-target checker was removed. One native module changes; the other19 native/support/legal payloads match public63. The exact same-certificate .2 APK is installed on Huawei via Mac USB; installed hash, retained app data, cold launch and Wi-Fi Console control/private reads pass while Mac-cabled. The owner key-only profile is privately imported. Complete matching source is retained locally; the temporary LAN server is stopped. The subsequently captured Huawei physical .2 session passes USB/identity/VIN/firmware/Stop; a CIM read failure remains under investigation. Pixel7 .2 regression, other targets and uniform qualification remain gaps. See `releases/consistent-baseline/HUAWEI_MDI_USB_CONFIGURATION_2026-10-05.json`.

Huawei remote control passed while Mac-cabled, including USB mode changes, fresh Wi-Fi connection, an ADB-service restart and private diagnostic reads. After the actual move to USB-host MDI operation, the Wi-Fi peer responds but debugging TCP refuses connections. This preflight does not qualify remote access in physical MDI operation.

Huawei .2 physical session now passes configuration1->2 (EBUSY16 then success0), fresh serial15ms, management login, fresh VIN, native firmware start and controlled Stop/USB cleanup. Original firmware later displayed a CIM NoComm error during the reported key-position read; earlier CIM DPID01 telemetry decoded ON. There is no recorded app/native crash during that operation. CANdi clock lag and CAN receive FIFO overruns are observed, but the specific error cause is unresolved. Complete owner-private logs are preserved; no raw identifiers/captures published. Pixel .2 regression, repeated CIM reads, live remote UI through switch and full uniform conformance remain pending.

## Classic MDI automatic bootstrap candidate — October 5, 2026

Private Huawei.6 removes connection-profile reads/imports and the JNI key argument. The shared OpenMDI core includes fixed management protocol material confirmed in both inspected general J2534/D-PDU Windows runtimes; it derives an adapter-specific key from a fresh validated serial and checks PDU2.5.33.154 before registration/START. Full uninstall/reinstall and exact installed bytes were verified with no profile, firmware card or sessions before first launch; prior data was backed up and never restored. Normal firmware download and Android USB permission were completed through the app. Physical fresh serial (861ms), automatic management login/START, fresh VIN, original Main Menu and operator Stop/USB cleanup pass. No vendor DLL, root or kernel network route was used. Configuration2 reinitialization (EBUSY16, temporary detachment/release of two interfaces, retry0/readback2) resolved the bulk data failure in this run. Earlier .3/.4 identity timeouts and .5 bulk OUT timeout remain failure controls; filter15/IGMP necessity and electrical cold-start behavior are not independently isolated.38 shared protocol tests,18 native tests, Android host suites and strict checks pass. This .6 behavior supersedes reuse-only configuration selection for the MDI binding. At that development checkpoint, public preview.63 was the older profile-dependent package; public preview.64 now supersedes it as recorded below. Pixel .6 regression, other targets/MDI firmware and the earlier CIM key-position failure remain unqualified. Receipt: releases/consistent-baseline/HUAWEI_FRESH_MDI_STARTUP_2026-10-05.json.

A second full uninstall/reinstall of the exact Huawei.6 APK also passed. Empty first-launch storage included no firmware, profile, sessions or shared preferences. Normal default firmware download matched the catalog card; a fresh Android USB-permission prompt was shown and allowed. Fresh serial729ms, automatic management login/START, fresh VIN, original Main Menu and Stop/USB cleanup passed again. Configuration2->2 reinitialization returned EBUSY16 then success0; the native worker was absent after Stop. Full logs are retained privately as a separate run. This establishes two fresh-app-install startup passes on the same powered fixture; electrical cold start and the previous CIM diagnostic failure remain separate. Repeat receipt: releases/consistent-baseline/HUAWEI_MDI_REPEAT_FRESH_INSTALL_2026-10-05.json.

## Public Classic MDI automatic initialization preview.64 — October 5, 2026

Exact signed nondebug ARM64 preview.64 passed full uninstall/install and empty storage, normal firmware download, fresh USB permission, automatic MDI bootstrap without profile import, fresh VIN, original Main Menu and clean Stop with USB closed/no native worker. All20 native/support/legal payloads match twice-fresh-tested Huawei.6. Five GitHub assets byte-verified; CI passed. Classic MDI PDU 2.5.33.154 on the same powered Huawei fixture. Pixel regression, electrical cold startup, MDI2/other firmware, previous CIM read error and full platform conformance remain unqualified. Security/SPA programming not repeated on public64. Receipt: releases/consistent-baseline/ANDROID_MDI_AUTOMATIC_INIT64_2026-10-05.json. The public website and updater now serve preview.64 after a verified Docker/Koyeb deployment; Facebook, Instagram and YouTube tester announcements are published; the full forum draft is verified in the browser.

## Nano read diagnostics CIM key-status candidate

The owner Huawei preview.64 trace entered the original Check Ignition Key Status
menu from the normal Read codes launch. The existing bounded CIM query was rejected
locally before USB transmission. The Android ARM64 private correction uses the
same exact three-query gate in normal Nano read diagnostics, preserves the normal
READY handshake and USB cleanup, and keeps seed, clear and unrelated control
requests excluded. Rejection guidance now offers a connection report without
requiring fresh security collection. Host Java and Nano Rust regression checks
pass; corrected physical operation and other targets remain pending. Receipt:
`ANDROID_NANO_READ_KEY_STATUS_2026-10-05.json`.

## Mongoose connection test beta — October 5, 2026

The optional App menu Mongoose connection beta follows the shared Android style in a separate experimental ARM64 APK. The contract records power-history labels, explicit USB permission, startup/channel tests, Stop and reviewed report sharing. Exact signed API28 ARM64 emulator checks and scoped UI review passed with simulated USB. Physical startup, VIN/DTC, full Mongoose guest transport, complete common-interface conformance and other targets remain pending. The existing reference scenarios and historical artifact qualifications are unchanged. Receipt: `MONGOOSE_ANDROID_BETA_2026-10-05.json`.


## Adapter chooser labels — October 9, 2026 source-only checkpoint before publication

The intended first chooser is **Select connection type**, followed by **USB adapters**,
**Bluetooth adapters**, and **Cancel**, in that order. The next chooser states actual
model availability. A connection-type label never makes an adapter compatible.
The frozen **0.2.23-ui-parity.3** Mac successor implements this wording for Intel and
Apple silicon; its exact successor package checks and publication remain pending.

Canonical Mac source adopts only the thin chooser wording and route order, Bluetooth
picker/help, and USB/About descriptions. Its runtime version and older Chipsoft USB
backend remain unchanged. The newer frozen Nano discovery, native workflow and
shared-status Launcher are deliberately not copied into this heavily modified checkout.
Canonical therefore explains Nano and MDI as unavailable in its current binding;
the separate frozen Mac preview can enumerate Chipsoft Pro and VCX Nano candidates.
Their identity, fresh VIN, cleanup and session gates remain unchanged.

OBDLink MX+ is **planned and unavailable**: no discovery, identity or command profile
has been implemented. Legacy Carista EVO / vLinker MC+ is limited to VIN and Trionic 8
engine reads; native I-bus diagnostics and SW-CAN wake remain unavailable in this
product path. The existing Classic MDI worker requires Windows; Mac has no MDI backend.
No original firmware, adapter command, security, API, SSA, transport or cleanup logic
is changed by these labels. Bluetooth startup identification still cannot launch
original firmware through a missing Bluetooth transport.

The shared `connection_types` contract records per-platform capability boundaries.
Historical qualified Android reference and other implementation selection strings
remain intact as predecessor metadata. Both Mac targets are source-implemented with
exact successor checks pending; Android ARM64/ARM32, iOS, Windows and Linux are planned
pending adoption and installed qualification. Existing checks, artifact/source pins,
owner-session evidence and full conformance/release gates retain their scope. The
pre-existing baseline target/schema/evidence gate errors are not repaired or bypassed
by this migration. No release, website, device or service operation occurs here.

## Published Mac adapter chooser preview — October 9, 2026

[0.2.23-ui-parity.3](https://github.com/djfremen/OpenSAAB-T2/releases/tag/macos-v0.2.23-ui-parity.3) is now published as an incomplete dev preview for
Intel and Apple silicon, with seven public assets verified against retained bytes.
The matching source archive and per-architecture manifests accompany both DMGs.
The public evidence is copied byte-for-byte to
[`MACOS_023_ADAPTER_CHOOSER_RECEIPT.json`](../../releases/consistent-baseline/MACOS_023_ADAPTER_CHOOSER_RECEIPT.json)
(SHA-256 `96d07221d1875e5d1083e587e5ff69deb56650155f8faa1d189cf6817912f09d`).
The [public download page](https://www.opensaab.com/static/t2/download.html) now serves this preview after a separate
verified website deployment (156 scoped checks, zero failures); Android catalog
and non-image deployment settings remain unchanged. This adds no vehicle qualification.

Both exact application bundles passed **83 native synthetic assertions each**:
29 shared status/USB binding, 42 Nano boundary and 12 AppKit modal assertions,
with zero failures and no vehicle transport or security-service access. Independent
source and package reviews recorded 2129 and 816 passing checks respectively,
with zero failures. Both bundles passed strict ad-hoc signature checks; they are
not notarized. Their 86 common resources and Info.plist are byte-identical.
Shared Rust/Nano/security/transport source remains unchanged from .2, but the
rebuilt Rust workers differ in bytes; this is not binary equivalence with .2.

The installed M4 app passed USB/Bluetooth/Cancel chooser routes, the Bluetooth
scope notice and disabled acceptance with no USB candidate. Intel reached a clean
Ready screen in separate staging; its complete chooser/menu/keyboard scenarios
remain pending. **Exact .3 DMG physical vehicle/adapter qualification remains
pending on both architectures.** The earlier Intel owner SPA Add and DTC evidence
keeps its original scope and does not qualify these new DMGs. Fresh Mac security
collection/service/import, SPA Remove and full interface conformance remain pending.

Canonical runtime remains **0.2.22** with its older Chipsoft-only binding; full
frozen Nano/status source adoption remains false. MX+ implementation and Mac MDI
transport remain unavailable; legacy Carista/vLinker remains VIN/Trionic 8 engine-only.
The other five targets still await source adoption and installed checks. Historical
.2 publication and owner evidence, MATRIX and the 16 existing global baseline
gate errors are preserved; no full-release or hardware gate is promoted.

## Mongoose VIN beta.2 — October 9, 2026

The separate experimental ARM64 successor preserves the contributor-tested startup/channel sequence and adds **Read vehicle VIN**. It uses one captured HS-CAN 1A90 request with adapter-owned ISO-TP flow control. A pass requires correlated adapter acceptance/completion, three ordered ECU frames containing a valid VIN, and acknowledged cleanup/USB release. Ignition-on/engine-off instructions and VIN-in-report disclosure appear before the test; each run clears previous state. The reviewed ZIP uses its unique filename in sharing subject/title to address the generic Drive name observed with beta.1. Android intent/provider behavior can be checked locally; the Google Drive receiver itself remains unverified. DTCs, SW-CAN wake, full original-firmware Mongoose transport, other targets and uniform conformance remain pending. The four beta.1 contributor reports keep their exact predecessor scope.

## Photo-readable crash evidence — October 13, 2026

Private Android preview.66 and Intel 0.2.24-reporting.2 are recorded in
`docs/CRASH_DETAILS_2026-10-13.md` and the matching receipt. The common local
snapshot retains rejected identity before startup, reports unknown values and
keeps USB/protocol cleanup independent. Android emulator checks pass; native
Intel and physical checks remain pending. Canonical runtime source adoption and
other targets are pending. Existing uploads retain their deployed schema; the
full identity/state block is in local crash logs. Public downloads are unchanged.

## Explicit fresh security collection paths — October 13, 2026

The urgent Android reference fix separates Manual collection and Guided collection
from processing and saved authorization. Get security access is reachable from Ready
and Actions. Manual sends no menu assistance. Guided selects recognized original
English menus for an operator-confirmed Saab 9-3 Sport (9440), MY2003–2012, with
the fresh VIN year; physical ignition prompts remain manual. Original firmware
owns the seed requests. Unknown context stops assistance with manual navigation
available; collection does not depend on external lookup, internet or password.

An explicit fresh request overrides old imported status. Exactly SSA is erased
on a disposable guest card; source card, surrounding firmware bytes and prior
receipt remain intact. Guest-before is used for fresh flash proof and source-before
for later source validation/import. The new isolated Android tests cover these
rules. Actual installed Nano/Chipsoft/MDI collection and processing remain pending.
This is a scoped Android oracle fix; the shared Rust migration, Android ARM32,
Windows, Intel/ARM macOS and Linux bindings/qualification are recorded separately.
Linux remains excluded from the October 13 vehicle campaign. See
`releases/consistent-baseline/SECURITY_COLLECTION_PATHS_2026-10-13.json`.

## Explicit Nano software reboot — October 13, 2026

The common Reboot Nano action belongs to the selected Nano adapter. Ready →
Actions exposes it only when Nano is attached; the Nano firmware Actions menu
also exposes it. An active session offers Keep session or Stop and reboot, with
confirmed cleanup before the separate adapter-only operation. Background, unplug,
timeout, changed target or incomplete cleanup cancels the handoff. Reboot requires
neither OEM diagnostic firmware nor debug privileges; security processing blocks
it. Existing identity/echo/reboot acknowledgement gates and zero vehicle writes
remain required. Preserve collection/card/receipt history and require explicit
Connect and start afterward. Software acknowledgement is not physical power-cycle
evidence. Android oracle tests do not qualify a physical reboot or other desktop
GUI bindings. Receipt: `releases/consistent-baseline/NANO_REBOOT_ACTION_2026-10-13.json`.

## Upcoming Auth_Status timestamp and VIN context — October 13, 2026

Owner-requested AUTH-STATUS-CONTEXT-001 is recorded as upcoming, not implemented
or qualified. The planned primary summary is Auth_Status plus associated VIN
last four, followed by Last SA data import with saved date/time/timezone, replacing
headline Fresh/Stale wording. Use only a successful matching imported_utc receipt;
validate the complete VIN and SSA hash, even if two VINs share the same suffix.
App reopen, adapter reboot and elapsed time never rewrite that timestamp. Missing,
invalid/future or mismatched context is not recorded/unknown. Prior imports for
changed data belong to explicitly labelled history. Imported-data status/time
does not establish current ECU authorization. Existing state/freshness fixtures
retain their scope; new UI and all-target verification remain pending. Receipt:
`releases/consistent-baseline/AUTH_STATUS_CONTEXT_UPCOMING_2026-10-13.json`.

## Guided collection with delayed vehicle context — October 13, 2026

A scoped Android oracle fix waits for verified context from the same native run
before constructing guidance. Creating the run directory before its vehicle file
no longer permanently cancels assistance. Manual takeover remains authoritative;
late context never restores guidance after operator navigation. The new regression
fails against prior source and passes with the three existing security suites.

Exact installed Pixel7 ARM64 0.1.0-preview.67-security.4, verified by APK hash,
automatically reached Get Security Access for the MY2004 9-3 Sport. Firmware
reported Reading all vehicle VINs OK and asked for LOCK. That run ended after
approximately five minutes with clean USB/protocol cleanup, no SSA erase/program
and unchanged erased guest SSA. Navigation is observed; completed collection,
processing and authorization remain unqualified. Other targets and shared-core
migration remain pending. Linux remains excluded from this campaign. Receipt:
`releases/consistent-baseline/GUIDED_SECURITY_CONTEXT_FIX_2026-10-13.json`.

## Exact Pixel7 Nano fresh seed collection — October 13, 2026

Owner-authorized guided fresh collection on exact installed ARM64
0.1.0-preview.67-security.4 reached original VIN and seed OK, then the complete
TIS2000 transfer-help prompt. The final714-byte PRE_AUTH block matches the
fresh campaign VIN and contains12 seed records with unfilled keys. Guest-before
is erased; guest-after equals both working data and the collection witness.
Original guest evidence records one SSA erase and828 programmed bytes. The
selected source card/SSA baseline remained unchanged. Explicit Stop completed
with confirmed USB closure, protocol cleanup and QUIT.

This verifies this exact Pixel7/Nano guided collection observation; the frozen
all-platform suite is not retroactively changed. External processing/import,
current ECU authorization, manual path and other target/adapter cells remain
pending. Evidence: `releases/consistent-baseline/PIXEL7_NANO_SECURITY_RETRY2_2026-10-13.json`.

## Nano imported data and idle resume failure — October 13, 2026

Exact Pixel7/Nano .67-security.4 OpenSAAB-only processing verified/imported the
714-byte response at17:15:20Pacific. Twelve keys are filled; VIN/seed records
are preserved, the original card backup verifies, and every byte outsideSSA
is unchanged. No fallback was selected. This is data import, not an ECU grant.

The automatic return requested after import failed: no new native run appeared
and the stopped collection screen persisted. Source inspection identifies idle
stopForSecurityHandoff calling cancel, setting cancelled=true before the resume
guard. The fix and exact repaired-build verification remain pending; no new
executable was installed. Imported data is retained; no repeat processing is
needed. Evidence: `releases/consistent-baseline/NANO_SECURITY_IDLE_RESUME_2026-10-13.json`.

## Android adapter rounds and bounded recording — October 13

Preview.70 fixes Nano's clean idle security handoff and imported-result recovery,
rejects obsolete async refreshes, and retains the Chipsoft disconnected reason
while clearing the live framebuffer and preserving its last screen evidence.
MDI uses the canonical shared bounded recorder: empty polling is skipped, reaching
128 MiB does not close transport, and omission/write errors are reported separately
from real transport error, pending tickets and final cleanup. Truncated evidence
cannot prove that a diagnostic command was absent. Eight nondebug Android AVD
regression suites, host checks, 256 MDI engine and six shared evidence tests pass.
The signed installed ARM64 package has its exact version and hash verified; new
hardware handoff/capture repeats remain pending. Other-target bindings/migration
and qualification gaps remain in status.json. Today’s earlier artifacts/owner
observations are in ANDROID_THREE_ROUNDS_2026-10-13.md; they do not qualify .70.

## Intel USB Refresh/replug preview — October 13, 2026

Private 0.2.24-usb-reconnect.1 Intel installs the thin persistent AppKit/IOKit
picker from clean source50fd8664c3cf00a7e2992887ea5d26259488eda3. Read-only scans
run inside modalPanel mode, update late arrival/removal in place, preserve selected
port identity/mode and revalidate Connect. OS USB absent, USB present/serial pending
and discovery failure are distinct; Refresh shows its check time. Existing shared
identity/VIN/policy/cleanup rules are unchanged in source. All8 rebuilt executable
bytes differ from reporting.2; no hardware result is inherited.

247 native Intel assertions pass (162 shell,42 Nano binding,29 parity,14 actual
modal lifecycle);98 installed file hashes and strict signature verify. Actual
Intel app menu/title/header and empty picker/Refresh layout are observed. Physical
same-process replug/fresh VIN/DTC CIM EXIT/key-status remain pending because the
OS still reports no serial device. Prior kernel port-power-off/controller-error
proof is retained; this picker does not recover a removed host port. ARM Mac has
14 synthetic source/modal checks only; canonical runtime source adoption and
other-target migration/installed/hardware work remain pending. Linux excluded
from current vehicle campaign. Receipt: releases/consistent-baseline/MACOS_USB_RECONNECT_FIX_2026-10-13.json.


## Apple silicon adoption of latest Intel preview — October 13, 2026

Private `0.2.24-usb-reconnect.2` packages the exact Intel reconnect/reporting
source natively for M4. All 98 installed files, eight ARM64 executable hashes,
full program/window/header identity and strict signature are verified; 247 native
checks pass. The actual attached Chipsoft Pro 1.5.2 appears in the persistent
picker and survives Refresh. The installed interface reads a fresh VIN, starts
Saab NAO 9.250 and reaches original Main Menu. Operator Stop exits the worker
without forced termination and leaves no serial owner; cold relaunch shows Ready
with saved history only. Original packages and selected firmware are preserved.

Physical unplug/replug, original DTC/CIM/ignition/security operations and complete
common scenarios remain pending on this exact artifact. Stop still retains the
last menu image until relaunch, an inherited shared-shell gap. CANdi was deferred
and did not open a firmware channel; its stop metadata stays USB/protocol unknown.
A separate packaged read-only VIN helper positively closed its actual channel and
serial handle after Stop; that does not qualify firmware transport cleanup or
GUI module operations. No uniform release, public publication or other-platform
pass is claimed. Receipt: `releases/consistent-baseline/MACOS_ARM64_INTEL_SYNC_2026-10-13.json`.

## FigCraft workspace refinement — October 14, 2026

The shared `layout.colors` and `layout.visual_design` specify the refined navy
workspace, section hierarchy, 48dp targets, framed firmware display and native
focus/hover/pressed feedback. Version 0.2.24-figcraft.7 is a private ARM64 Mac
preview based on the exact installed USB-reconnect/reporting source. Its new
colors come from packaged metadata. No adapter/protocol/security behavior is
reimplemented. The editable FigCraft file and renderings are in `design/`.

Android ARM64/ARM32, Intel macOS, Windows, Linux and iOS source/installed
adoption remain pending. Canonical runtime-source reconciliation is separate.
The current preview and its exact tests belong in
`releases/consistent-baseline/FIGCRAFT_DESIGN_2026-10-14.json`; this refinement
does not declare all-platform conformance or inherit prior hardware passes.

## Local MCP preview — October 14, 2026

OpenSAAB 0.2.24-mcp.2 implements local MCP stdio and a native AI control
(MCP) action on branch `codex/mac-mcp-native-menus`, using the qualified
FigCraft Mac source. The exact installed Apple silicon bundle passed offline
original-menu inspection/navigation, real soft keys, cancellation, stale state
and ownership rejection, manual input and Disable MCP checks. One call
traversed Diagnostics and eight year moves in 2.59 seconds without screenshots.
The canonical checkout records the shared `ai_control_mcp` direction, but its
unrelated runtime changes still require source reconciliation. Intel Swift
typechecking passes; Intel installation/execution, live hardware and other
platform bindings remain pending. See `MACOS_MCP_2026-10-14.json` in the
consistent-baseline release records for exact hashes, evidence and scope.

## Chipsoft compatibility guidance (October 14, 2026)

`chipsoft-compatibility-contract.json` specifies passive identity-based advice. The Android private candidate shows observed versus tested firmware and an explicit official vendor link after cleanup. Only an unambiguously older Pro gets update advice; matching/newer/unknown firmware and unsupported models remain distinct. No adapter capabilities, protocol commands or automatic updates change. Android software evidence and the private installer are recorded separately from pending physical and other-target qualification. Canonical runtime reconciliation remains pending.


## Saab 148.000 welcome and deferred CANdi — October 14, 2026

Private 0.2.25-candi-startup.1 fixes the shared welcome gate for all ten runnable catalog cards. It verifies the Saab card manufacturer record plus the active LCD version/ENTER token, retaining deferred construction and original byte-verified guest loader guards. Exact installed M4 .1 passed ten firmware boots, three recorded-key dependency failures and normal Swedish download/Main Menu/model-year/EXIT/Stop navigation. The old Swedish replay reproduced the processor halt at 0x171C6. Intel package signatures/hashes are verified; native Intel Nano/Ventura execution is pending. Current contributor card hash/English capture and other packaged platform adoption remain pending. Stop still retains the last framebuffer until relaunch; no uniform conformance or physical adapter qualification is claimed. Receipt: `releases/consistent-baseline/MACOS_CANDI_FIRMWARE_STARTUP_2026-10-14.json`.


## ARM64 CANdi startup rebuild and required password policy — October14

Private `0.2.25-candi-startup.2` retains the scoped .1 guest welcome fix. Only
Apple silicon is rebuilt for the owner vehicle test; Intel promotion is deferred.
The shared security worker and thin macOS UI/HTTP binding implement the canonical
case-sensitive OpenSAAB password requirement before processing, fallback or new
processed-response import. Wrong/blank/case-changed/malformed/cancelled requests
fail before file or HTTP effects; remembered credentials are revalidated. Exact
installed worker, native HTTP and isolated Keychain tests must accompany the
candidate. Android reference and other installed targets are not upgraded or
qualified by these checks; full common and physical Nano qualification remain
pending. Historical packages and previous test evidence are preserved.

## Mongoose personal I-Bus comparison — October 14, 2026

The scoped Android ARM64 candidate `0.1.1-preview.3` follows the newly reviewed native Body/Driver Information and sleep-then-ignition captures. It adds an explicit no-HV-wake CIM read and a one-HV-wake CIM read, each requiring fresh raw DPID01 plus a complete correlated CIM VIN and acknowledged cleanup/USB release. Contributor-reported ignition/sleep context is retained; zero/intermediate tuples remain unknown. A missing status read preserves a separately verified VIN as a partial result, never an overall pass. The personal probe has no security processing/import or full firmware backend. Its fresh-launch welcome and installed name/version/architecture/date use packaged metadata. Original beta.1/beta.2 evidence is preserved; physical Mongoose, sleeping-bus wake, raw-channel lifetime and all other targets remain pending. The signed candidate requires its own installed emulator and unavailable-security evidence before publication.

Personal Mongoose publication evidence: `MONGOOSE_ANDROID_CIM_BETA_2026-10-14.json` (SHA-256 `7fe5f9cddeee25ab264f45e7e486030cce6e33b511ccb531ceb071d89df31929`). Signed `0.1.1-preview.3` passed 298 installed assertions/27 simulated USB fixtures, direct native visible-footer checks and portrait/landscape/large-font review. Exact-package unavailable-security evidence and deployment completion gate passed; all six public asset hashes match. Hardware/sleep/electrical wake and uniform qualification remain pending.
