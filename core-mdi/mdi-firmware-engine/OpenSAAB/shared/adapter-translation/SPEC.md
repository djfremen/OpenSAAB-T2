# Adapter translation implementation specification

October 3, 2026. This specification describes the implemented source migration
for OpenSAAB's shared CAN and diagnostic reply behavior. Its readers are the
application, adapter, vehicle-emulation and evidence agents. Use this contract
when extending their work so that USB, wireless and virtual targets produce
equivalent protocol results without acquiring conflicting transport ownership.

The migration is **partial and qualified offline**. Existing wired and vLinker
source paths now consume common frame validation and decoders. A bounded host
transaction API is implemented and tested, but existing diagnostic workflows have
not all been rewritten around that API. New retail adapter drivers, physical
SW-CAN qualification and installed-platform regression remain pending.

## Canonical source and evidence

The canonical package is `opensaab-adapter-translation` version `0.1.0`, at
`OpenSAAB/shared/adapter-translation`. It uses Rust 1.93, edition 2021, MPL-2.0,
no runtime dependencies and `sha2` as a build dependency. `tech2-emu` consumes
a reviewed snapshot of this package. This canonical file is the authoritative
specification; the snapshot copy must be refreshed through the import tool.

The implementation is independently authored and consolidates OpenSAAB's own
VIN and GM report decoders. No proprietary application routine, ECU catalog or
third-party parser source was imported. [EVIDENCE.json](EVIDENCE.json) records
the evidence and limitations behind each selected behavior:

| Reference | Behavior selected | Evidence boundary |
| --- | --- | --- |
| OBDocker GM path | Collect A9/81 reports separately from ISO-TP | Static inspection; full fixture and physical validation pending |
| Carista native routines | Preserve GM records, bound multipart assembly, select FC explicitly, separate STN/MIC profiles | Native replay and static findings; EVO and complete termination chain pending |
| STN documentation and Rogue33 observations | Keep STN controls in an exact adapter driver | Attributed MX+ Android observations do not establish another adapter or iOS result |
| WiCAN and MeatPi components | Distinguish raw CAN from assembled payloads | Five parser fixtures; native pin-1 route unqualified |
| BlueDriver generation and assets | Declare capabilities per firmware/profile; preserve status/failure bytes | Retail generation mapping and complete parser replay unresolved |
| Existing Chipsoft, Nano, J2534 and vLinker | Retain guest traffic, electrical context, completion provenance and command guards | Prior receipts remain bound to their exact artifacts |
| Primary ISO-TP documentation | Enforce length/sequence rules and one flow-control owner | Transport rules do not establish hardware support |

Owner evidence, acquisitions and private logs remain in `tech2-emu/docs` and
`tech2-emu/audit/2026-10-02`. The implementation overview is
`tech2-emu/docs/UNIVERSAL_ADAPTER_TRANSLATION_2026-10-03.md`. The current
verification receipt is `tech2-emu/audit/2026-10-03/adapter-translation/verification.json`.

## Ownership and layer boundaries

The shared package validates normalized frames and interprets replies. Adapter
drivers own USB/Bluetooth/Wi-Fi I/O, command dialects, bus routing, settings,
readback, deadlines for their I/O and release of native resources. Application
workflows own request intent, connection lifetime, cleanup and publication of
results. Original Tech2 guest firmware continues to construct its own requests
and flow-control frames.

`CommandFamily` names Chipsoft, Nano, J2534, ELM, STN, MIC, BlueDriver and Virtual.
These are metadata, not implemented command compilers or evidence of support.
The package does not substitute ST commands for VT commands, open an adapter,
switch a pin, wake a bus or issue a diagnostic request.

The guest and host paths are distinct:

| Path | Input and output | Ownership rule |
| --- | --- | --- |
| Original guest | Original classical CAN frames through a validation facade | Guest owns requests and FC; no host assembly or reconstruction |
| Host ISO-TP | Raw reply frames or explicitly declared assembled payloads | Exactly one qualified host or adapter FC owner |
| Host GM report | Addressed raw81 records and A9 control replies | Raw reception; automatic CAN FC must be disabled by the driver |
| Virtual target | The same raw guest boundary or authored host observations | Origin remains model/replay/firmware evidence, with separate physical comparison |

## Frame and observation contract

[src/lib.rs](src/lib.rs) defines the common envelope.

| Type | Fields and behavior |
| --- | --- |
| `Bus` | `HsCan`, `SwCan`, or `Other(u16)`; bus identity accompanies the frame |
| `RawFrame` | Bus, CAN ID, 11/29-bit flag, RTR, DLC and original bytes |
| `Packet` | Either `Raw(RawFrame)` or `IsoTpPayload { bus, id, extended, data }` |
| `Observation` | Session ID, normalized monotonic `at_us`, optional original device timestamp, origin and packet |
| `Origin` | Physical, replay, behavior model or firmware emulation |

Classical frames require valid 11/29-bit IDs and DLC 0 through 8. Data frames
must contain exactly DLC bytes; RTR frames contain no data. Validation rejects
malformed frames rather than padding, truncating or correcting them.
`Packet::guest_frame()` rejects assembled ISO-TP payloads because their original
segmentation and PCI bytes are absent.

The caller supplies a common monotonic session clock for every observation,
including observations ultimately ignored by the transaction. Device timestamps
are preserved separately; normalization and uncertainty require their own
evidence. The package does not infer wire latency, a shared clock or capture
completeness. Origin is metadata: the caller and experiment ledger must prevent
mixed synthetic/physical evidence from being reported as a physical result.

## Capability and profile contract

[src/profile.rs](src/profile.rs) implements `Capabilities` and `SessionProfile`.
Capabilities declare qualified buses, raw receive/transmit, assembled receive,
adapter FC and simultaneous buses. The caller supplies these declarations; this
package does not probe hardware or independently certify them.

`SessionProfile` selects bus, mode, representation and `FlowControlOwner`:

| Mode | Accepted profile |
| --- | --- |
| `GuestRaw` | Raw receive/transmit and guest-owned FC |
| `HostIsoTp` | Host-owned FC with raw RX/TX, or declared adapter-owned FC with an available receive representation |
| `HostGmReport` | Raw RX/TX and FC owner `None`; the driver must actually disable automatic FC |

Invalid combinations fail. `validate_concurrent()` also rejects different-bus
profiles when simultaneous buses are not qualified. This check does not reserve
an adapter or schedule bus switching. Connection arbitration and settings
transitions/readback remain workflow/driver responsibilities.

## Bounded host transaction API

[src/host.rs](src/host.rs) implements one `Session` per ECU, bus and session ID.
Its `Config` contains request/reply CAN IDs, address width, protocol, profile,
limits and host FC parameters. `Protocol` selects either ISO-TP with an active
service and expected positive prefix, or GM A9 with a distinct report CAN ID.
Construction validates the profile, addresses, deadlines and protocol limits.

Default limits are 512 payload bytes, 128 GM records, a one-second idle deadline
and a ten-second absolute deadline. Callers may select other bounded values.
Idle/total deadlines must be nonzero and idle must not exceed total. ISO-TP
capacity is 1 through 4095 bytes; GM capacity is 1 through 2048 records.

`feed(Observation)` first checks clock/deadlines, then selects the session,
bus, CAN ID and address width. Valid unrelated observations return `Ignored`
without extending the idle deadline. Selected pending/progress updates the idle
reference; pending can never extend the absolute deadline. A backward clock,
deadline, malformed selected reply or rejected request fails the transaction.

The API returns these effects:

| `Action` | Meaning and caller duty |
| --- | --- |
| `Ignored` | No accepted transaction progress |
| `Pending` | Matching request remains pending; continue within deadlines |
| `Progress` | Accepted incomplete assembly/report progress |
| `SendFlowControl(RawFrame)` | Send through the existing guard only when this host owns FC |
| `Complete(ResultData)` | A complete payload or complete GM record collection; workflow cleanup/publication gates still apply |

States are Active, Complete, Failed and Cancelled. Terminal sessions reject
subsequent feeds/ticks. `cancel()` makes an active session terminal. Callers
must invoke `tick()` when no frames arrive, handle I/O failure, and close/restore
their driver after every terminal outcome. Errors currently use `String`; there
is no stable typed error-code or serialized API contract.

This API assembles replies and returns FC effects. It does not construct requests,
segment host transmissions, enforce sender pacing, reserve connections or restore
adapter settings. Production end-to-end adoption of these responsibilities is a
separate integration step.

## ISO-TP receive behavior

[src/isotp.rs](src/isotp.rs) implements classical CAN with normal addressing.
Single frames contain 1 through 7 payload bytes. First frames must contain eight
CAN bytes and declare a total payload of at least 8 within the selected capacity.
Consecutive frames must supply the required bytes and the expected rolling
sequence number, including wrap. Padding is excluded from the completed payload.
Truncated, duplicate, unexpected or out-of-sequence frames fail the receiver;
completion/failure is terminal.

The host session emits FC after a first frame and after each configured block of
incomplete consecutive frames. Block size zero means no further block FC.
Adapter ownership emits no host FC. Valid STmin encodings are `00..7F` and
`F1..F9`; reserved values fail configuration. Actual transmission and pacing
remain the caller's responsibility.

Matching `7F <service> 78` is pending before raw multipart assembly starts;
other NRCs, a wrong service or a pending interruption of active raw assembly fail.
Assembled-payload mode accepts only declared adapter ownership and validates the
expected positive prefix and bounds without fabricating raw frames.

`VinReply` preserves the existing `feed() -> needs_flow_control` and `vin()`
helper API. It expects the eight-byte frames for a 19-byte `5A 90` reply and a
17-character VIN excluding I/O/Q. Saab-specific prefix checks remain with callers.
This compatibility helper has no bus/session clock envelope and retains its
existing immediate negative-response handling; it is not a replacement for the
bounded `Session` API.

CAN FD, ISO-TP extended/mixed addressing and 32-bit extended lengths are outside
the implemented scope. A 29-bit CAN identifier is not ISO-TP extended addressing.

## GM A9 and raw81 report behavior

[src/gm.rs](src/gm.rs) implements `Report` and `Dtc`. The direct collector retains
`receive`, `receive_for_ids`, `is_complete` and `finish`. Default control/report
IDs are 7E8/5E8; callers may supply exact other IDs. The direct collector has no
bus/session/deadline envelope; `host::Session` supplies those checks when adopted.

Report records begin with `81` and have five through eight bytes. Bytes 1 and 2
encode the DTC, byte 3 is the failure type and byte 4 is raw status. Preserve
these bytes rather than deriving a vendor description or universal status label.
The collector does not deduplicate records or impose a vendor catalog.

Completion requires `81 00 00 00 FF` with optional remaining CAN padding.
An all-zero DTC/failure field with another status is an invalid end marker.
Control replies handle `03 7F A9 78` as pending and other A9 NRCs as rejection.
Record capacity, malformed records, invalid end and traffic after end/failure
are errors. `finish()` requires explicit successful completion.

Complete empty, incomplete, rejected and failed reports remain different outcomes.
A prompt, quiet bus or timeout never establishes an empty success. Full workflow
publication still requires the existing diagnostic stop and confirmed transport
release where that workflow requires them.

## Source integration actually completed

| Consumer | Change made | Remaining work |
| --- | --- | --- |
| `tech2-emu` host helpers | VIN and GM types re-export shared implementations | Complete workflows retain their existing sequencing/deadlines |
| `tech2-emu` guest | J2534, Nano USB, Chipsoft USB and virtual attachments use `TranslationBackend` | New installed/hardware regression |
| Canonical OpenSAAB and iOS emulator | Same facade on existing J2534/Nano/Chipsoft USB and Chipsoft serial attachments; shared VIN helper | Target/artifact qualification |
| Shared vLinker workflow 0.1.3 | Shared identifier ISO-TP receiver, VIN helper and GM collector; exact translation dependency | Pixel 7 EVO preview.43 bounded host reads verified; other installed targets and physical MC+ regression pending |
| VirtualSAAB | Existing guest backend can attach through the common facade | Independent model/firmware correctness and physical comparison |
| Carista, STN/OBDLink, WiCAN, BlueDriver | Contract and evidence available | Additional driver/profile implementation and qualification |

`TranslationBackend` is thin wiring around the existing `Backend` trait. Controller
0 maps to HS-CAN, 2 to SW-CAN and 1 to `Other(1)`; unknown controllers fail.
It validates guest TX and driver RX metadata, passes original bytes and complete
`CanTransmission` context through, and closes its inner backend once.
Driver tickets and completion labels are unchanged. USB write, compatibility
device reply, software loopback and virtual completion do not become physical
CAN ACK evidence. Driver errors remain errors; the facade adds no recovery or
complete-bus recording guarantee.

The full original firmware emulator, UI and OS transport code have not been
consolidated into this package. The independently implemented VirtualSAAB
verifier remains independent so it can expose disagreements rather than merely
reuse the client decoder. No additional vendor driver or settings manager was
implemented by the migration.

## Version pins and change procedure

| Source | Version | SHA-256 of its source inventory |
| --- | --- | --- |
| Adapter translation | 0.1.0 | `87277736b9e9d59469a092a1ab6e38905adcbfa58103e2d17ed0318f36f101c7` |
| vLinker workflow (current private source) | 0.1.3 | `41d0d75ffe4d99a811cb4dcc0e3cfb56ea76889f808df52a9b48305bbdc9b760` |

October 4 organization changed the private workflow source inventory while retaining
package 0.1.3 and identical wire behavior. Model-owned MC+/EVO profiles share the
MIC recipe. Prior installed receipts retain inventory `d34004c8c619947b9edb61ea710c5c59791ccda87d425bb060bdce2a8f73d65e`;
they do not qualify the new source pin. See the [organization checkpoint](../../emulator/docs/adapters/ADAPTER_ORGANIZATION_2026-10-04.md).

`SOURCE_SHA256` inventories Cargo.toml, build.rs and runtime Rust sources.
The vLinker inventory also includes `TRANSLATION_SOURCE_SHA256`, binding the
translation inventory hash. Build guards reject mismatched source and transitive
pins. Tests, specification, README, evidence metadata and lockfiles are outside
that runtime inventory; retain their own verification/checkpoint records.
These local pins are review controls, not signed releases or proof of authorship.

Edit canonical source first. Review behavior, update package version/inventory
and the workflow's transitive pin as needed, deliberately refresh consumers and
lockfiles, then rerun affected tests. Import the tech2-emu snapshot with:

```sh
python3 scripts/import_adapter_translation.py \
  --source /home/user/Documents/Projects/OpenSAAB/shared/adapter-translation
```

The current importer refreshes metadata for an identical runtime pin and refuses
to overwrite a differing snapshot. Preserve and review the existing revision
before replacing it. Do not independently patch the snapshot or recreate these
decoders in Swift, Java, C# or Python production bindings.

## Verification and qualification limits

| Check | Result and scope |
| --- | --- |
| New shared contract tests | 12 passed; synthetic profiles, addressing, FC, pending/deadlines, assembly, completion and failure cases |
| New guest facade tests | 3 passed; preservation and malformed-metadata rejection |
| tech2-emu library/binaries and relevant integrations | 218 passed, 9 ignored |
| Canonical OpenSAAB library | 145 passed, 7 ignored |
| Shared vLinker workflow | 24 passed |
| New translation crate Clippy | Passed with warnings denied |
| Canonical desktop binary compile | Passed with existing dead-code warnings |
| iOS embedded core compile | Passed with existing unused `PathBuf` warning |
| Changed source and changed dependency negative builds | Both rejected by the expected pin guards |

Counts overlap in logic and are not independent physical results. Two synthetic
vehicle-lab fixture end records initially used status zero; they were corrected
to explicit FF completion and the suite passed. The first iOS featureless check
failed because firmware storage needs `embedded`; the correct configuration
passed. Initial failure logs and final receipts are retained.

The migration performed no live hardware I/O, provider/JEV request, app
installation, vehicle write or release. Installed Android preview.37/iOS23
qualification remains tied to its prior workflow version and receipts. Source
adoption/qualification for every declared target is recorded in
`OpenSAAB/releases/consistent-baseline/status.json`; pending targets must not
inherit another target's pass.

## Required follow-up by project agents

Application agents should consume the canonical decoders through existing shared
workflow bindings and preserve fresh identity, connection ownership, Stop,
disconnect, cleanup and saved-report semantics. Adoption of `host::Session` must
include silent-bus ticking, guarded FC sends and driver failure/cleanup tests.
Install and qualify exact resulting artifacts before promoting platform status.

Adapter agents should implement exact STN/MIC/other command profiles below this
contract, with settings readback/restoration and explicit physical routing/wake.
Qualify raw versus assembled input, FC ownership, buffering, bus switching versus
concurrency, cancellation and completion provenance. Shared reply decoding does
not resolve Nano cold initialization or prove passive Chipsoft CANHacker capture.

Vehicle-emulation agents should generate original raw frames with explicit model
origin and compare independent expected behavior against the shared client.
Preserve held-out real recordings and separate model conformance, firmware
execution and physical agreement. CPU startup success does not establish ECU
boot, firmware-generated CAN or correct bus behavior.

Evidence/JEV agents should bind source pin, adapter/firmware/profile, platform,
run ID and capture method to every analysis. Reanalyze preserved originals when
the decoder changes; retain prior interpretations. The twelve EVO/MC+ runs with
Chipsoft references remain planned and NV. JEV may rank authored investigation
choices from reviewed evidence; it does not certify outcomes or control CAN.

For the overall OpenSAAB specification, the defensible current claim is:
**common decoding and frame contracts are implemented and adopted in source;
universal end-to-end adapter support and new physical/platform qualification are
still incomplete.**

October 3 Pixel 7 EVO qualification is a separate exact-artifact receipt:
`releases/consistent-baseline/CARISTA_EVO_PIXEL7_2026-10-03.json`. The shared
workflow 0.1.2 identifies EVO/MIC3413 separately from MC+ and reuses the common
VIN decoder. This extends Android host HS-CAN VIN/voltage qualification only.
EVO SW-CAN, ECM probes, iOS transport and complete original-firmware support
remain pending; the earlier migration and simulator receipts are unchanged.

The separate preview.43 receipt `CARISTA_EVO_PIXEL7_ECM_2026-10-03.json`
extends physical Pixel 7 EVO MIC3413 V2.0.84 qualification to seven Trionic 8
identifiers and a complete current/history mask12 code report. Workflow 0.1.3
retains model-specific operation profiles and requires the GM end marker,
diagnostic stop and native release before saving codes. SW-CAN, other installed
platforms, other EVO probe firmware and original-firmware wireless remain pending.
