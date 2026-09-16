# Owner security-access pilot — 2026-09-16

This ARM64 owner test adds operator-entered, expiring bearer authorization to the
OpenSAAB primary request only. It contains no server credential, vehicle grant,
security database or calculation handler. Existing local diagnostics remain
unchanged. ARM32 is not rebuilt for this pilot.

Open **Security access authorization** on the main screen to enter or remove the
private authorization. It is encrypted with Android Keystore in noBackupFilesDir;
backups are disabled. The server validates expiry, revocation and vehicle
entitlement independently. The APK does not confer authorization.

The processing confirmation describes the restricted owner pilot's raw 714-byte
input storage (including VIN/security data), deletion scheduled after one day,
seven-day outcome records, operator access/deletion contact and separate Bojer
fallback consent. No bearer is sent to Bojer, placed in request files or logged.
Unauthorized, expired or wrong-vehicle requests do not trigger fallback.

After processing, the existing SSA validation and atomic working-card import
remain mandatory. HTTP success and import are not proof of vehicle acceptance.
A fresh collection and owner-observed vehicle acceptance are required for each
vehicle. No vehicle test is implied by a software or historical fixture test.

This is a limited owner pilot, not public account sign-in or public security-access
availability. Keep the temporary private authorization out of screenshots,
support reports, chat, source and release artifacts. This build is installed
locally for the owner and is not advertised in the public release manifest.
