# OpenSAAB cross-platform product rule

The owner's standing “McDonald's fries” requirement is the same OpenSAAB
interface, setup, controls, features and adapter behavior on Android ARM32,
Android ARM64, Windows, macOS and Linux. Android is the current interface
reference, not a separate product direction.

The canonical contract is maintained in the companion OpenSAAB repository:
`emulator/ui/interface-contract.json`, `emulator/ui/CONFORMANCE.md` and
`releases/consistent-baseline/status.json` (local sibling `../OpenSAAB`). Read
those before changing interface or setup behavior. Update the shared reference
and track every affected target when the product changes; do not silently fork
labels, workflows, firmware-key semantics or diagnostic-result meanings.

Preserve optimized ARM32 startup, deferred CANdi and existing adapter buckets.
Keep OS permissions and adapter transports platform-specific internally while
preserving the same user workflow. Each installed architecture needs its own
evidence; no inherited pass from ARM64, another adapter or a standalone helper.
Missing functionality remains an explicit gap, not a cosmetic placeholder pass.

These rules do not require extra approval for already-authorized work.

## Development preview identity

Each changed preview must use a distinct full version with prerelease/revision
and architecture in its installed program name, displayed identity, main title,
installer and manifest. Read displayed versions from packaged metadata. Retain
artifact hashes and historical packages; never rename a running diagnostic app
or alter a published signed bundle. This candidate explicitly adopts the rule
on Android ARM64 as well as macOS. Other target adoption remains recorded pending.
