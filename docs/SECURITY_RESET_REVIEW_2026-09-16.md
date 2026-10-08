# Security reset review — September 16, 2026

## Observed sequence (Pacific daylight time)

- 15:40:53: private server event confirms processing. Its archived 714-byte input contains 13 populated seed records, with unfilled key and security-code fields. It is not an erased card.
- 15:45:01: Pixel reset receipt confirms Clear Offset.
- 15:45:25–15:47:24: next native Chipsoft session. User photo shows “SSA inconsistent”, scode.c line 466. The final console failure is “Incomplete USB write”; cleanup reports false. The evidence does not establish that the USB failure caused the SSA assertion.
- The reset removed the current processing receipt as designed. The prior per-session evidence remains private on the phone. No vehicle-access acceptance is established by these observations.

## Byte operation versus integration

NoMoreGlobal's Clear Offset uses offset 0xFE0000, size 714, fill 0xFF. SsaCardReset matches it. Its transaction backs up the complete card, verifies the backup, checks all bytes outside the region are unchanged, and atomically replaces the working card only while stopped.

The regression is in native session setup: SSA flash command/status/read-array emulation was enabled for dedicated seed collection, but not NativeManual with native CANdi. The Android Full native control screen uses NativeManual. A cleared card can require original-firmware initialization; with raw card writes disabled and no SSA flash model, that initialization is dropped. This is a concrete emulator defect consistent with the photo, not yet a reproduced full firmware assertion.

NativeManual with a native CANdi link now enables the same bounded, in-memory SSA flash model. It does not enable raw file writes or change any vehicle-command permission. The source card is still not persisted by the native guest. Ordinary API processing still requires a fresh guest-written snapshot, valid VIN/header and nonempty seed records.

## Checks

- Regression test compares the same program/status/read-array sequence with SSA flash disabled/enabled on an erased region: old path reads FFFF, fixed path reads the guest-written header; surrounding bytes remain unchanged; raw writes remain disabled.
- Native mode tests verify normal manual sessions select the model without enabling seed/audible/symbol command modes.
- Android SSA validation explicitly rejects a fully erased block and a valid VIN/header block without seeds.
- Existing reset test verifies the complete 32 MiB card and its backup.

## User flow and evidence

Clear once, before collection. Collect with original firmware and follow every key-position prompt. Process and load the fresh response. Retry the requested operation without clearing again. API processing and importing do not establish vehicle acceptance.

The reset now archives the preceding processing receipt alongside its backup before invalidating the current receipt. Reset copy explicitly warns that clearing after processing removes the saved response.

Pending: physical vehicle retry to establish whether the SSA assertion is eliminated and to verify vehicle acceptance. Private phone evidence and server data must not be added to the public repository.
