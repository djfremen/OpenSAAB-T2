# Security collection menu navigation

The Chipsoft Get security access action uses the current connection's vehicle identity to select the firmware's model year, Saab 9-3 Sport (9440), All, and Get Security Access. The initial implementation recognizes the supported NG 9-3 platform and model years 2003–2012; unrecognized identities or menus remain available for manual navigation.

Navigation reads captured original-firmware menu text, waits for a stable screen, sends one ordinary keypad event, and waits for the screen to change. It selects function keys from their labels rather than fixed function-key positions. User keypad/gesture input ends automatic navigation. A stalled menu times out to manual navigation without closing the firmware session. Ignition/key-removal prompts remain operator-controlled. API submission and card import behavior are unchanged.

Validation: JVM tests cover 2004 and 2008 navigation, alternate function-key positions, stale screens, unknown platform/menu text, manual takeover, timeout and an unaccepted key. Signed ARM64 preview.11 is intended for Pixel testing. Actual vehicle navigation still needs a live test; no live vehicle commands were sent during development tests.

## Preview 12: faster selection and automatic return

The real original-firmware test confirms that `(4) 2004` and `(8) 2008` are VIN year codes, not number-key shortcuts: pressing those number keys leaves the highlight at 2012. The regression test also confirms that paced ordinary arrow events select both target years correctly. The app now polls captured menu changes every 100 ms while navigating, with a 50 ms stability check, instead of waiting two seconds between rows. Each next action still waits for the displayed row to change; there is no blind burst of queued keys.

After a successful import has released the card-edit lease and workflow gate, the UI shows “Security access ready — restarting firmware” and starts full native firmware on the same identified adapter connection. It does not ask for another VIN/start confirmation. It retains the loaded timestamp in the next session. If the app is in the background, it resumes once when the app returns; Stop or adapter detach cancels that pending restart. Failed API/import operations never reach the restart callback. No offset reset occurs during restart. “Ready” describes successful response import; the app does not invent a vehicle access-granted response.

The live preview 11 run selected 2004 / 9440 and imported the API response successfully. Preview 12 automatic return still needs the next live run.
