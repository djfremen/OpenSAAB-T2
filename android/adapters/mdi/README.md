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

Before claiming the transport interfaces, read the active configuration with the
standard USB GET_CONFIGURATION request. Reinitialize configuration 2 even when
it is already active so the bulk data endpoints are reset before a new session.
If selection fails with EBUSY, force-claim the current configuration's
interfaces to detach a bound kernel driver, release every successful temporary
claim without reconnecting the kernel driver, then retry the selection once and
verify its readback. Android10's Java `releaseInterface` explicitly reconnects
the driver; temporary releases therefore use the granted descriptor's native
USBDEVFS_RELEASEINTERFACE ioctl alone. Normal final Android release is unchanged.
The native configuration ioctl retains errno for the local Console and receipt;
only EBUSY triggers driver detachment. Linux usbfs rejects
SET_CONFIGURATION while an interface is owned, including a redundant selection
of the current configuration. Cancellation and partial failure release temporary
claims; failed release prevents another selection. No RNDIS or vehicle command
is sent by this step. Connection reports distinguish `USB_CONFIGURATION` from
`USB_OPEN`; the local release receipt records configuration IDs and cleanup.

This change follows Huawei EVR-L29 Android 10 preview.63 evidence: nine successful
USB opens followed by failed configuration selection, before any interface claim
or native handshake. Private huawei.1 repeated that failure in the owner's photo.
The regression model reproduces the Android10 release/reconnect failure; its
no-reconnect counterpart passes. Private Huawei.6 also passed a fresh install:
configuration 2 was reinitialized after EBUSY16, two temporary interfaces were
detached/released, and readback confirmed configuration 2. Fresh serial, automatic
management login, VIN, original Main Menu and controlled Stop/USB release passed.
Earlier reuse-only .3/.4 sessions received no announcements; .5's group join
exposed a bulk OUT timeout. Host tests alone do not qualify hardware.

The shared OpenMDI classic-MDI bootstrap includes the fixed management protocol
material observed in both pinned Windows J2534/D-PDU runtimes. It is not a
per-owner password or ECU Security Access key. Android neither reads nor imports
a connection profile, and the JNI interface accepts no serial or key input.
No Windows program, vendor DLL, root or Android network driver is used.

After RNDIS initialization with packet filter15 and the startup drain, the carrier
joins the observed discovery multicast group using IGMP, then retrieves a fresh
serial from the observed IPv4 multicast `225.1.1.1:8194` announcement (`0x86d`,
version byte 7, LE serial at body offset 9). It validates Ethernet/IP/UDP framing,
checksums, source address and record bounds. Retrieval has an eight-second deadline
and remains cancellable; no management or vehicle request starts before it
succeeds. Generic USB descriptor serials and saved serials are never used as
management identity. The shared core derives the adapter-specific Blowfish key
and requires inventory PDU version 2.5.33.154 before registration/START. Other
versions and MDI2 remain unqualified. Firmware setup is still required and follows
the common software-selection/download workflow.

A fresh VIN read and confirmed discovery-channel cleanup precede firmware startup.
VIN lookup is local in this module; no remote lookup or security processing is
triggered by connection. Security processing reuses the common explicit-consent,
stop, validation, SSA import and resume workflow. Physical ignition prompts are
never acknowledged automatically. Security collection uses the separate `seeds`
authority; normal connection uses `full` original-firmware authority.

Stop keeps the carrier available while the guest closes both links and releases
its owner. It then disables RNDIS filtering, cancels/reaps USB input, sends HALT
and releases Java interfaces. The carrier leaves the discovery multicast group
before disabling filtering. Cancelled permission callbacks cannot reopen a
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
