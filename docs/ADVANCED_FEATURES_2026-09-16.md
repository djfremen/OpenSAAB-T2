# Advanced tools and diagnostic shortcuts

The launcher and Chipsoft session now offer Adv. Features: restricted mode, adapter detection, USB tests, device check, security password, and offset reset. Restricted mode continues to apply to the next connection. Existing session guards and the offset reset confirmation remain in place.

Launcher buttons use the same 48dp rounded buttons and 8dp spacing as the firmware soft keys. Portrait details can scroll without hiding EXIT; landscape ARM32 retains its firmware display and separate control rail.

Explicit Read DTC, Clear DTC, and Engine Data choices pass a shortcut to Chipsoft. Ordinary Start does not. Shared menu navigation uses the current session's identified year/platform, stable captured firmware text, recognized function labels, a bounded timeout, and manual takeover. DTC shortcuts select All → Diagnostic Trouble Codes → the requested action. Engine Data selects Engine → Engine Control and recognized data-display labels. Unrecognized menus remain under manual control. No automatic confirmation/ignition responses are added. Security collection keeps its established navigation and processing flow.

Validation: pure Java fixtures cover 2004/2008 menu routing, remapped function labels, stale screens, missing identity, timeout, manual takeover, and stopping at task confirmations. Actual vehicle testing of the three new shortcuts remains necessary; menu fixtures do not prove ECU communications or coverage of every firmware/language.

## Review builds and device checks

- Signed ARM64 `0.1.0-preview.15` / `100015` and ARM32 `0.1.0-headunit.9` / `100009`, app source `1abde09`.
- Pixel 7 updated in place to preview.15; firmware and saved settings retained. All launcher controls visible at the phone's current display/font settings; full previous-vehicle details open on tap.
- Pure Java request/transport/security/navigation tests and build-profile tests pass.
- Android emulator session-layout, keypad, and security suites pass. Layout suite re-run after the compact vehicle-summary adjustment; all six advanced tools, consistent touch targets, main row spacing, persistent EXIT and synthetic 1024×600 head-unit display checked.
- Visual review performed on the physical Pixel: launcher and Advanced Features. Firmware splash checked in offline emulation with no connected adapter.
- Not published to the website. New diagnostic shortcuts still require connected-vehicle testing; ARM32 device runtime testing is pending.

## Vehicle history follow-up

ARM64 preview.16 / 100016 and ARM32 headunit.10 / 100010 (source 5b875dc) add the saved VIN-observation date as Last connection, plus vehicle-scoped `auth_status`, recorded pre/post-auth time, and advisory Fresh/Stale. No connection or authorization timestamp is fabricated from app startup or file modification times. Missing/mismatched receipts show timestamp unavailable. History refreshes on foreground/focus and once per minute; it never gates a session.

Pure Java history/transport/security tests and Android session-layout instrumentation pass. Preview.16 installed in place and visually checked on Pixel 7: saved connection and post-auth timestamps, Stale indication, all launcher controls and EXIT visible. These remain review builds, not website releases.

## Session shortcuts and naturally entered security collection

Review builds preview.17 / 100017 and headunit.11 / 100011 (source 2144f01) keep Get security access, Adv. Features, Read DTC, Clear DTC and Engine Data outside the portrait details scroll area. Shortcuts can join recognized current firmware menus; unknown task screens are left alone. Physical firmware control cancels automatic navigation.

Full-control security collection is adopted when the captured firmware screen shows Checking Security Access with the VIN/seed sweep. A menu label or generic TIS-required page alone does not count as fresh collection. At the transfer prompt the pinned action becomes Process security data and uses the same run's native snapshot, existing consent dialog, API handling, baseline checks and automatic restart. No repeated collection session is introduced. The manual adoption resets when the run changes. Old on-disk post-auth status is replaced with collection progress while the guest collects new data.

Validation: pure Java navigation/receipt/SSA tests; Android security instrumentation explicitly exercises manual full-control sweep → transfer prompt with zero automated keys, no session restart and no API request. Session-layout and keypad suites pass, including visible pinned shortcuts and EXIT. ARM64 installed in place on Pixel 7 after user confirmed the vehicle session was finished; settings/firmware retained. ARM32 built. A real-vehicle manual collection/API round trip on this revision remains to be tested; builds not published to website.
