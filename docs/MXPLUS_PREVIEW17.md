# MX+ touch Enter correction · preview.17

Preview.16 connected the primary Bluetooth route to original MX+ firmware after verified VIN discovery and cleanup. Its first run then exited4 after one touch Enter: MainActivity wrote the offline command `enter` into `native-key.txt`, whose parser accepts encoder bytes or stop. The native report recorded the invalid mailbox command; the MCP path already sends encoder0x10.

Preview.17 keeps that handoff and encodes touch Enter as0x10 for MX+ sessions. Offline logical Enter is preserved. Firmware keys, consent, packet rules, adapters and worker source behavior are unchanged. Test the actual touch control after normal connection, then navigate a scoped read-only operation. Preserve preview.16 artifacts and failure logs. Other architectures/platforms and iOS remain unqualified.
