# Firmware workspace — September 17, 2026

The launcher and native Chipsoft session now reserve the main workspace for the original, aspect-preserved Tech2 framebuffer. A compact header opens vehicle/security details; S1–S4 and Exit/Keypad/Actions remain outside scrolling panels. Start is only shown when the session is stopped.

Actions opens a bounded, scrollable sheet for Get security access, Read DTC, Clear DTC, Engine Data and saved DTC reports. Clear DTC requires confirmation before handing off to the existing navigation flow. Manual security collection still updates the compact status and changes the first Actions item to Process security data at the transfer prompt. Processing/import and automatic restart remain in the existing security workflow.

The app menu contains Stop (distinct from firmware Exit), firmware selection, adapter/advanced tools, report issue, console, updates, support and controls help. Exact timestamps, VIN details and auth_status remain in the details view. Unknown history does not occupy multiple persistent rows. Keypad and Console open overlays rather than shrinking the underlying firmware area. Gesture instructions appear in Keypad and Help. Expand screen hides the header; Actions remains available to restore it or open the app menu.

The layout uses measured window dimensions, not model names or ABI. Windows at least 720 dp wide and wider than tall use a 248 dp side panel. Portrait windows use a compact header. The same code is used by both architecture builds. Existing orientation policy is retained.

Validation before packaging:
- Shared request/security/card-state/menu-navigation suite passed.
- Android layout instrumentation passed at the Mate report's 1080 × 2172 pixels / 480 dpi, with normal and 1.3 font scale. Measured fixtures also cover 360 × 660, 393 × 780, 800 × 480 and 1024 × 600 dp. EXIT, Actions, menu and status remain fully visible.
- Keypad instrumentation passed all 23 key callbacks, gesture/cancellation checks, repeat opening/dismissal, overlay sizing, and all three existing activity layouts.
- Security instrumentation passed natural manual collection through Process security data in the same session, compact Ready to process status, receipt scoping, prompt discrimination and dialog dismissal. No USB/API/card operations were performed by these UI tests.

These are UI and synthetic workflow checks, not renewed physical head-unit or connected-vehicle certification. Installer removal behavior is a separate issue and is not changed here. No public release catalog is updated by this source change.
