# October 14 Android integration preview

`0.1.0-findings.6` / versionCode `100077`, ARM64, private development candidate.
The source checkout is `/Users/mini4/Documents/Projects/android-findings-20261014`
on branch `codex/android-findings-20261014`. Android Studio is open on this checkout.
The recorded Android preview.70 and Chipsoft-guidance source are the baseline.

## Integrated findings

- Error checking: Nano corruption remains terminal until a fresh stream;
  Bluetooth receive failures invalidate buffered data, while explicit close evidence
  remains independently observable. The existing report, cleanup and health flows remain.
- AI control (MCP): explicitly enabled, token-authenticated app-owned local socket;
  original guest LCD menu text and observed selection bars; canonical packaged keys;
  session/revision/input-consumption guards; cancellation on manual input; bounded
  exact-label navigation; current health separate from historical OS exits.
- Chipsoft: observed Pro firmware version and passive compatibility guidance,
  retaining unknown/rejected identities and offering the existing fixed official links.
- Recovered core: OEM config round trips/includes/environment paths/backups,
  pSOS file ABI, separate ATA/FAT capture storage and damaged-FAT recovery,
  firmware Data Display names/units/values and connection-state distinctions.
  Empty capture storage requires the explicit `--capture-allow-empty` option;
  no fake imported capture/config file is created.
- Localized Saab 148.000 welcome detection is adopted into both packaged core paths.
- MX+: Android RFCOMM effects use the existing serialized STN2255 HS/SW backend,
  owned by original CANdi with no inferred electrical ACK. Use App menu →
  MX+ firmware session. The fixed bench adapter fingerprint remains scoped to
  the previously researched MX+; universal firmware/device compatibility is unqualified.

The recovered core is used for offline/MX+ sessions. Chipsoft/Nano retain their
existing Android engine, with the corruption/welcome/menu-publication changes.
MDI retains the exact preview.70 native module and firmware worker. MDI has no new
MCP key/menu binding or recovered-core runtime qualification in this candidate.
ARM32 and other targets remain pending; no public release was changed.

## Checks and remaining hardware work

Gradle APK payload checking and Lint pass. Saved native regressions and installed
AVD tests cover authentication/revocation, actual original firmware menus,
MCP Enter → Main Menu → Diagnostics → Model Year, stale/manual-input rejection,
and normal Stop. These are software/AVD observations, not physical adapter passes.

The Pixel 7 was initially reachable over paired wireless ADB but locked. It later
disconnected; reconnecting to its observed endpoint returned No route to host.
The final APK has therefore not been installed on the Pixel and the requested
MX+/bench ECM session is pending. Reconnect/unlock the Pixel, attach the powered
bench ECM to MX+, select the Pixel in Studio, and run this exact candidate.
Verify installed version and pulled APK/native hashes before reading VIN/ECM
information/codes and testing Stop/reconnect. An ECM-only fixture may lack CIM
key-position input; report that separately from transport disconnection.

## MCP bridge

Enable App menu → AI control (MCP) in the foreground session. The local stdio
server is `scripts/android/opensaab-android-mcp.py`; provide one explicit already
paired ADB serial and a private token file (`chmod 600`). The pairing code is
shown by the app. Private debuggable testing may obtain the same code with
run-as; production pairing does not require run-as. Stop or leaving the app
revokes the endpoint. Do not publish pairing codes, vehicle data or raw captures.

Artifacts, source/evidence hashes and all prior attempt APKs are retained at
`/Users/mini4/.local/share/opensaab/android-findings-20261014/`.
