# Classic MDI startup identity — 5 October 2026

Preview.63 retrieves the management serial from a fresh adapter announcement after
RNDIS initialization and startup drain, before management login or a vehicle request.
Saved serials and USB descriptor serials are not inputs to native initialization.
The dedicated selected USB carrier validates Ethernet multicast identity, IPv4 and
UDP framing/checksums, the observed source, record length/opcode/version and nonzero
serial. Retrieval has an eight-second deadline and Stop remains available.

The owner Pixel7 / Android16 classic-MDI fixture retrieved the correct serial in
542ms with a schema-2 key-only profile and no saved SN, then read a fresh VIN and
started the original firmware. Normal Stop confirmed native firmware teardown,
RNDIS filter-off, reaped USB input, HALT and Java interface/connection release.
With the profile removed, retrieval completed in878ms, with zero management/TCP
connections and zero vehicle requests. USB was released before the visible
connection-key import prompt. The private test APK and native hashes are pinned
in the adjacent JSON receipt. Exact public-package checks belong to its release
receipt; the private test is not presented as an exact public APK test.

The base key still requires explicit local import and is not bundled. Schema2
contains adapter family and the base key; legacy schema1 works with its serial
ignored. This does not establish unique base keys per unit or universal MDI
provisioning. Fifteen native tests and strict Clippy checks pass, including bad
checksums, wrong source/version/opcode, truncation, bounds and zero serial. Shared
Java host regressions pass. Only the MDI carrier native payload changed from62.

MDI2, other physical adapters, complete removal of both power sources, other
platforms and uniform product qualification remain pending. SPA/security
programming was not repeated. Existing recorder and partial-DTC-export limits
remain unchanged. Raw VINs, serials, keys and captures stay private.
