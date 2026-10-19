# OpenSAAB 0.1.1-preview.19 ARM64


### MX+ confirmed code-clearing option · preview.19 source · 2026-10-19

The owner requested a write option as well as read-only diagnostics. Preview.19 binds the existing shared ClearDtc profile to the dedicated Android MX+ worker with a distinct authenticated role. Normal Connect and start remains read-only. A fresh-menu preflight intercepts Clear Codes before key enqueue for touch, hardware keyboard and MCP. Stay read-only preserves the current session. Proceed records explicit consent, stops the old session and requires both confirmed native protocol cleanup and Bluetooth socket release before restarting the original menus in code-clearing mode. The original firmware erase confirmation remains required; no key or request is automatically replayed. App menu exposes the same consent flow.

The common policy is unchanged: exact service04 and A9/81/0A clearing operations on supported routes, plus existing reads. Counter reset is mutually exclusive; security, programming and unrelated writes remain unavailable. Cancel, background, stale callbacks and unconfirmed cleanup cannot start the new mode. Host tests cover profile isolation/original bytes on both buses, authenticated-role mismatches, option conflicts and once-only consent. Hardware-keyboard Enter is normalized to native encoder0x10; offline Enter is retained.

This is source adoption only. Exact Android ARM64 package/install and Fire/MX+ tests are pending. Other targets have no inherited adoption or pass; iOS stays on hold. A completed test must retain the before/after DTC inventories, actual original erase request, ECU response, original result and cleanup. An active fault can remain or return. Preserve prior packages and receipts.
