# MX+ primary connection route · preview.16

Preview.15's primary Bluetooth button performed VIN discovery only. Its working native read/reset sessions used separate App menu entries. Preview.16 connects the normal **Connect and start** route to the existing native MX+ read worker after verified fresh VIN discovery and confirmed Bluetooth cleanup/owner release. It uses the same explicit device and secure/insecure choice without a second picker.

Only the actual shared discovery model `obdlink_mxp` may hand off. A one-shot foreground generation rejects cancellation, background state, obsolete selections and duplicate callbacks. Failed discovery, unconfirmed cleanup, a held owner or missing native binding never starts firmware or offline emulation. Maintenance consent, packet rules, STN scheduler, USB workers and internal authentication remain unchanged. Offline buttons explicitly clear any prior MX+ selection.

The retained discovery worker records discovery as a standalone operation and still has a generic missing-binding terminal message internally. The Android platform consumes its released receipt to start the separately evidenced firmware operation; an internal discovery-only record is not proof of end-to-end firmware failure or success.

Software guards and exact Fire primary-route physical evidence must be recorded separately. Other adapters, ARM32 and other platforms inherit no qualification. iOS implementation remains on hold. Preserve preview.15 packages and receipts.
