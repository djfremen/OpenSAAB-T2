# Apple Bluetooth effects

`AppleBLETransport.swift` is compiled directly into the iOS and macOS native apps.
The iOS Xcode source reference uses the sibling canonical OpenSAAB checkout;
macOS packaging uses this file from the canonical root. The shared Rust
`opensaab-vlinker-workflow` package remains the authority for adapter identity,
firmware gates, commands, framing, timeouts, VIN/ECM interpretation and reports.

Carista EVO and vLinker MC-IOS both use service **18F0**, TX **2AF1** and RX
**2AF0** with response writes. EVO service and characteristic properties were
observed through CoreBluetooth on M4 on October 4, 2026; the MC-IOS profile was
observed on the earlier M4 bench. Bluetooth names are candidate hints, and
peripheral UUIDs / Classic addresses select individual endpoints. Shared
**VTI / VTVERS** replies distinguish the products and gate their CAN profiles.

Discovery is bounded and never connects automatically. The app opens only the
explicitly selected endpoint. Response writes are chunked and acknowledged;
notification bytes that arrive during a write wait until the write completes.
Cancellation stops scanning and waits for the native disconnect callback.
There is no automatic reconnect, transport fallback or command replay.

macOS also provides native paired Classic SPP through `MacBluetoothTransport.swift`.
Authorization is requested before paired-device enumeration, which runs away
from the AppKit thread. The app opens the observed SDP Serial Port channel,
confirms native writes and confirms channel/device release. A Classic connection
already owned by another process is rejected without closing that connection.

Scope: fresh VIN / voltage and explicit HS-CAN Trionic 8 ECM information and
current/history mask12 code reads. SW-CAN, module programming, security and
original-firmware wireless/CANdi transport are separate. A compiled iPhone or
Intel Mac artifact is not a physical hardware qualification. See the exact
artifact receipt in `releases/consistent-baseline/APPLE_BLUETOOTH_2026-10-04.json`.
