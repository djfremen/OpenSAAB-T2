# Device testing

## Huawei Mate 20 X — 2026-09-15

User reported successful installation of the public APK and supplied a photo showing the first-run welcome screen. Device: EVR-L29, EMUI 12.0.0, Kirin 980, 6 GB RAM. The screenshot does not establish the underlying Android version.

Confirmed by user report: APK installs, the welcome/setup screen opens, and OpenSAAB successfully reads fault codes with Chipsoft on the Mate 20 X. This establishes a successful diagnostic read on this device; no session logs were collected in this exchange.

USB connection note: the user reported needing to enable USB debugging before the successful read. Preserve this as an observed setup condition, not a proven universal requirement or confirmed root cause of the earlier cable issue. Exact working cable configuration was not reconfirmed.

Still unverified on this device: VIN capture, report export, security access, code clearing, programming, extended-session stability, and operation with USB debugging disabled. The firmware download method was not separately confirmed.

The welcome screen previously showed a text-only OpenSAAB label. It now uses the same BrandHeader and approved launcher artwork as the main screen. This source change requires a future APK update; the published preview.2 APK is unchanged.
