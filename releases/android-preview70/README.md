# Android ARM64 preview.70 — October 13 adapter round fixes

This preview incorporates the Chipsoft, VCX Nano and Classic MDI testing work.

- Fresh security collection is reachable directly with Manual and Guided choices.
  Manual leaves original menus to the owner; Guided waits for verified vehicle
  context. Both leave physical ignition prompts to the owner. Fresh collection
  clears only the 714-byte SSA on a disposable card copy, preserving the selected
  software, previous imported data and evidence until a validated import.
- Nano processing after a clean Stop no longer cancels the return to firmware.
  The imported result supersedes that collection's historical PRE snapshot.
  Stale asynchronous refreshes cannot restore the previous workflow state.
  Cancellation, unconfirmed cleanup, changed adapter and background gates remain.
- Reboot Nano is a separate adapter-only action. It stops and confirms cleanup,
  requires the exact adapter acknowledgement and never automatically reconnects.
- Chipsoft USB detachment is displayed as an interrupted operation, with the
  disconnected cause and last screen evidence retained, rather than operator Stop.
  The ignition status detector is unchanged. Direct OTG succeeded in the owner's
  repeat testing; this software update does not establish a cause for the hub loss.
- Classic MDI no longer ends diagnostics when its raw capture reaches 128 MiB.
  Empty polls are not recorded. Capacity or later write failure is recorded as
  capture truncation; real transport failures, pending tickets and cleanup remain
  separate. Capture absence is not proof an operation did not occur.

Today: Chipsoft owner reports covered read/clear codes, ignition-key status and
SPA Add. Nano saved partial DTC results, streamed ECM values with engine off,
collected fresh seeds and imported a validated result. The later MDI run saved a
partial vehicle-code report and reached original SPA Add completion/security-check
OK. Its later stop did not confirm USB/protocol cleanup. Do not interpret these
as comprehensive code clearing, all-module security grants or qualifications of
this new artifact. MDI's first run failed at the software recording boundary.

Software regressions and installed package checks are recorded in the attached
release receipt. New handoff/capture fixes still need a live repeat on the signed
release. Android ARM32, Windows and both Mac architectures do not inherit these
ARM64 test results. Linux is excluded from the owner's current vehicle campaign.
Auth_Status with VIN last four and a last-import timestamp remains an upcoming
feature. No vehicle security material, raw VIN or private session log is published.

Install over the existing ARM64 app to retain software/settings/security data.
The name/header/manifest identify OpenSAAB 0.1.0-preview.70 ARM64, code 100070.
The original firmware/support, Nano/Chipsoft/connection and MDI USB module payloads
retain their preview.66 bytes; the MDI firmware bridge is rebuilt with the shared
bounded recorder. The named matching-source ZIP carries all component sources,
including the rebuilt bridge; retained components have not been independently
reproduced byte for byte. No adapter firmware update or expanded model compatibility.
