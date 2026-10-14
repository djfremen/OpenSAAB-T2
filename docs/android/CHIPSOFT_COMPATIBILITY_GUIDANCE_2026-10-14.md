# Chipsoft compatibility guidance — October 14, 2026

Private Android ARM64 candidate **0.1.0-chipsoft-guidance.1**, version code100071, based on published preview.70 source7451e9fa. The packaged program name is **OpenSAAB 0.1.0-chipsoft-guidance.1 ARM64**. Preserve public packages and other active worktrees.

The passive Rust crash formatter emits bounded compatibility advice. Android displays it after cleanup, with detected versus tested identity, a reviewed-report action and an explicit official vendor link. Older Pro firmware gets the Pro1.5.2 package. Matching1.5.2, newer/unknown firmware and Mid/Lite are different cases; none is diagnosed as an older-firmware crash. Android does not use the Windows J2534 driver. No compatibility gate, adapter command, automatic upload/retry/update or firmware flash is added.

The public driver/firmware package verified on the manufacturer's site is1.5.2, dated January13,2021. The official forum references a1.5.3 test driver without a verified public release/package. Do not claim a newer tested adapter version or tell the earlier reporter that his quoted1.52 proves old firmware. Obtain actual CRASH DETAILS.

Rust host checks and synthetic Android native/UI tests pass. Physical adapter/vehicle checks and other targets are pending. This candidate has not been publicly released. The canonical UI contract is emulator/ui/chipsoft-compatibility-contract.json; canonical runtime reconciliation remains pending.
