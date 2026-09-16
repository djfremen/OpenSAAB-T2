# Security collection menu navigation

The Chipsoft Get security access action uses the current connection's vehicle identity to select the firmware's model year, Saab 9-3 Sport (9440), All, and Get Security Access. The initial implementation recognizes the supported NG 9-3 platform and model years 2003–2012; unrecognized identities or menus remain available for manual navigation.

Navigation reads captured original-firmware menu text, waits for a stable screen, sends one ordinary keypad event, and waits for the screen to change. It selects function keys from their labels rather than fixed function-key positions. User keypad/gesture input ends automatic navigation. A stalled menu times out to manual navigation without closing the firmware session. Ignition/key-removal prompts remain operator-controlled. API submission and card import behavior are unchanged.

Validation: JVM tests cover 2004 and 2008 navigation, alternate function-key positions, stale screens, unknown platform/menu text, manual takeover, timeout and an unaccepted key. Signed ARM64 preview.11 is intended for Pixel testing. Actual vehicle navigation still needs a live test; no live vehicle commands were sent during development tests.
