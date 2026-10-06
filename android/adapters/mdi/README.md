# MDI Android transport module

The main OpenSAAB package includes this module and the packaged original-firmware
MDI bridge. Selecting the USB candidate opens `MdiUsbActivity` in the same app;
there is no runtime dependency on the OpenMDI research APK.

`MdiUsbActivity` binds Android USB permission and interface ownership to the shared
`SessionWorkspace`, `Tech2Controls`, LCD and key pumps, security workflow and passive
DTC report observer. `MdiNative` exposes the carrier through JNI. The Rust module
owns RNDIS initialization and an isolated userspace Ethernet/TCP carrier; the
pinned OpenMDI core owns management and D-PDU records. The packaged firmware bridge
retains original guest CAN commands and flow control.

The current owner preview is ARM64 and the observed classic-MDI configuration:
configuration 2, control/data interfaces 0/1, full-speed bulk endpoints 81/02.
Generic RNDIS USB IDs remain candidates, not model identification. Other models,
descriptors, physical cold reconnect and other architectures require qualification.

The connection profile is local to `getNoBackupFilesDir()/mdi/connection-profile.json`.
It contains schema 1, adapter_family `classic_mdi`, an unsigned nonzero adapter serial
and a base64-encoded 56-byte management key. Import is explicit in App menu.
Credentials are neither bundled nor logged. The owner fixture was provisioned
locally; no Windows program, vendor DLL, root or Android network driver is used.

A fresh VIN read and confirmed discovery-channel cleanup precede firmware startup.
VIN lookup is local in this module; no remote lookup or security processing is
triggered by connection. Security processing reuses the common explicit-consent,
stop, validation, SSA import and resume workflow. Physical ignition prompts are
never acknowledged automatically. Security collection uses the separate `seeds`
authority; normal connection uses `full` original-firmware authority.

Stop keeps the carrier available while the guest closes both links and releases
its owner. It then disables RNDIS filtering, cancels/reaps USB input, sends HALT
and releases Java interfaces. Cancelled permission callbacks cannot reopen a
session. Unexpected firmware termination does not become connection success merely
because cleanup succeeded. EXIT navigates guest menus; Stop belongs to App menu.

Normal sessions omit raw USB/Ethernet recordings. An explicit debug-only
`mdi_capture` option retains the bounded private collector path. Native sessions
remain finite. The experimental full-data receive-flag compatibility inherited
from the recorded classic-MDI bridge remains scoped to this preview; it does not
qualify universal RTR fidelity or lossless guest delivery.

Automatic MDI diagnostic menu shortcuts are pending. The current module exposes
the original firmware menus and common security/report components. The owner-operated private main4 session now has matching fresh collection/API
import, original SPA security-check OK through ECU finished and key-status proof.
Public-package checks, separate decoded ECU grant and broader hardware coverage
remain separate. See docs/adapters/MDI_ANDROID_PREVIEW.md.

Build the native module and matching bridge with `scripts/android/build-mdi-module.sh`.
Set `OPENSAAB_MDI_MODULE` and `OPENSAAB_MDI_BRIDGE` to those outputs when building
the main APK. Packaging checks require both exact ARM64 payload hashes and the
dependency notices. Keep the bridge's matching engine source and core dependency
pin with the APK; do not substitute an older research executable after changing
the protocol core.
