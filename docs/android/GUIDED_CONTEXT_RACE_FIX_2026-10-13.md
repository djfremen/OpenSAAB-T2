# Guided security collection waits for its fresh vehicle context

Pixel7 / Nano /0.1.0-preview.67-security.3 got a verified fresh VIN matching the
private VIN…2017 pin, prepared its disposable collection card and reached Main
Menu. Guided navigation cancelled without sending any menu key. The selected
firmware metadata is English NAO9.250 and fresh VIN year2004; unavailable language
or VIN is not the explanation. Remote manual takeover reached Model Year only;
no fresh seed collection completed. The original firmware run ended at its
five-minute boundary, exit3, with clean USB shutdown and zero diagnostic writes.
No security API processing occurred. Original evidence is retained privately.

A new session directory is visible before card preparation and VIN-file publication
complete. SecurityAccessView constructed its navigator on the first observation
with null identity; the navigator immediately cancelled and was never reconsidered
when verified context arrived in the same directory. A new isolated instrumentation
regression reproduces that exact sequence and fails against the previous source.

The fix defers construction while fresh context is absent, then creates navigation
once the same run supplies verified identity. Manual mode/takeover and unsupported
context remain respected; cancellation is not repeatedly restarted. A pending
context message and bounded no-VIN diagnostic events explain waiting/readiness.
Physical ignition prompts, original seed requests, source/receipt preservation,
network consent and native payloads remain unchanged.

Private ARM64 candidate0.1.0-preview.67-security.4 /versionCode100069. It retains
all preview66 native/support bytes and the previous manual/guided and reboot fixes.
The previous signed packages and failing installed-artifact evidence are preserved.
Shared Rust migration/other platform bindings and physical collection qualification
remain separately pending. Linux is excluded from the current car campaign.
