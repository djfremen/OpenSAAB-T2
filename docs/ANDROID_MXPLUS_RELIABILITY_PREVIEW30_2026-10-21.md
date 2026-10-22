# Android MX+ connection deadline — 0.1.1-preview.30

The physical Fire's installed preview.28 logs contain six secure RFCOMM connect
timeouts before any adapter or vehicle command, interleaved with two successful
VIN/retained-socket handoffs. A separate, fixed-owner read-only control probe on
the same Fire and MX+ subsequently established secure SPP in 10.314 seconds.
The production eight-second watchdog would terminate such a valid attempt.
This is a demonstrated cutoff defect; it does not establish the cause of every
earlier failure or qualify the vehicle/security workflow.

The connection watchdog now permits one attempt up to 20 seconds. It still closes
a pending socket on timeout/cancellation, preserves authenticated SPP by default,
and adds no retries, pairing changes or vehicle-request replay. A completed
connection disarms its timer atomically, including a callback already queued.
The startup transcript/result now retain connection phase, elapsed time, deadline,
selected device, bond state and discovery state even when there is no ready event.
The existing two-second write deadline and retained native handoff are unchanged.

Read-only adapter queries reported OBDLink MX+ r3.1.3 / STN2255 v5.12.4,
BT24H R15 / 200915E_Scantool, pairing mode 0 (automatic), native sleep control,
UART sleep 600 seconds, UART wake enabled and last sleep/wake UART/UART.
No reset, sleep disable, firmware update, pairing configuration or vehicle command
was sent. Startup-banner handling and a voltage-comparison `>` character initially
interrupted the research probe's readout; both historical probe revisions remain
retained. Revision 3 captured the complete configuration and trigger replies.

Sources: Android's Bluetooth connection guide documents SDP lookup and roughly
12-second platform failure timeout:
https://developer.android.com/develop/connectivity/bluetooth/connect-bluetooth-devices
OBD Solutions FRPM F, sections 8.11, 8.12 and 15.3, documents the read-only queries,
sleep triggers and waiting for the wakeup prompt before commands:
https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf

The common user workflow is unchanged; this is an internal Android transport
correction. Shared Java source applies to ARM64 and ARM32; each installed target
requires its own evidence. Other OS Bluetooth bindings are not changed or
qualified. Native component bytes are explicitly retained from preview.28 with
their original source revision, not rebuilt or relabeled as new native code.
Build, installed verification, repeated reconnects, asleep/awake conditions and
physical vehicle qualification must be recorded separately in local receipts.
Raw research and adapter identity remain local and are not publication inputs.

Packaging checkpoint: local preview.29 was rejected by Android because its resource
table was compressed. No preview.29 was installed. Preview.30 retains each base
entry's compression, including stored/aligned resources.arsc; the Java transport
change is unchanged. Exact signed artifacts and the rejection remain local.
