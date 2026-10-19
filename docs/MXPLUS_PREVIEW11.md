# Android MX+ preview.11 source scope

MX+ firmware startup in preview.9 configured the adapter and received passive CAN, then crashed in the recovered research boot. Preview.10 was an uninstalled host experiment: fidelity boot removed that crash but did not advance the welcome screen with CANdi active. Historical packages and evidence are preserved.

Preview.11 gives MX+ a separate ARM64 legacy-engine worker, `libtech2_mx.so`. It imports the same STN backend source used by the recovered runtime and shares its frame boundary and bounded ignition-key query. It does not change the six pinned preview.1 USB/connection executables. The recovered engine remains the offline worker. Firmware/card files are not replaced. No vehicle replies or diagnostic requests are synthesized by the host.

Read mode stays read mode. An explicit, confirmed T8 counter-reset session adds only the captured original HS7E0 `02 AE 14 00 00 00 00 00` request. The authenticated Java owner handshake distinguishes read and reset profiles. DTC clearing, seed/key submission and other writes remain blocked. Physical testing must first observe zero RPM, save the counters, use the original reset confirmation once, and verify ECU reply plus readback. Zero before/after does not prove a nonzero counter changed.

The host fixture boots the Fire's exact card through Main Menu, Diagnostics year list, EXIT and operator Stop with the raw-byte peer. It exercises no ECU. Native exit 3 on manual Stop is retained as historical behavior, not claimed as a vehicle or cleanup pass. All six retained workers are verified by SHA-256 during packaging. Exact source, signed APK, installed identity and private physical-session evidence are recorded separately.

The two MX+ secondary App menu entries remain a shared-interface integration gap. Android ARM32 and all desktop/iOS bindings and physical qualification are pending. This private candidate is not published and does not qualify a uniform release.
