# Advanced tools and diagnostic shortcuts

The launcher and Chipsoft session now offer Adv. Features: restricted mode, adapter detection, USB tests, device check, security password, and offset reset. Restricted mode continues to apply to the next connection. Existing session guards and the offset reset confirmation remain in place.

Launcher buttons use the same 48dp rounded buttons and 8dp spacing as the firmware soft keys. Portrait details can scroll without hiding EXIT; landscape ARM32 retains its firmware display and separate control rail.

Explicit Read DTC, Clear DTC, and Engine Data choices pass a shortcut to Chipsoft. Ordinary Start does not. Shared menu navigation uses the current session's identified year/platform, stable captured firmware text, recognized function labels, a bounded timeout, and manual takeover. DTC shortcuts select All → Diagnostic Trouble Codes → the requested action. Engine Data selects Engine → Engine Control and recognized data-display labels. Unrecognized menus remain under manual control. No automatic confirmation/ignition responses are added. Security collection keeps its established navigation and processing flow.

Validation: pure Java fixtures cover 2004/2008 menu routing, remapped function labels, stale screens, missing identity, timeout, manual takeover, and stopping at task confirmations. Actual vehicle testing of the three new shortcuts remains necessary; menu fixtures do not prove ECU communications or coverage of every firmware/language.
