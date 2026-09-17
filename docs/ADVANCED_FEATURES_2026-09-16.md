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
