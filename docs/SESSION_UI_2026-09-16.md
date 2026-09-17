# Session controls and security state

The Chipsoft screen uses equal-width controls within each row, explicit 8 dp gaps, and a 48 dp minimum touch target. The start/stop/back row is 56 dp tall so wrapped labels retain the same height. Details and Clear offset share one row in a separate security panel. Session information can scroll on constrained portrait displays while the firmware display and EXIT remain outside that scroll. The ARM32 landscape action rail remains separate.

Security labels match NoMoreGlobal_dotnet8_v8's BinAnalyzer.DetermineSsaStatus and SsaStatus.GetDisplayString: [INIT_AUTH], [PRE-AUTH], [POST-AUTH], and [INVALID]. Classification reads the 714-byte working-card region and follows NoMoreGlobal's VIN/key/original-seed field checks, including padded VIN handling. This display classification is separate from the stricter existing API input/reply validation. It does not introduce any new vehicle verification gate or change reset/import/automatic restart behavior.

Details distinguish Pre-auth collected, Post-auth received, and Post-auth written timestamps. Existing receipts remain readable. Private key/seed values are not printed in the UI or new tests. Processing history remains scoped to the current identified vehicle; a changed working card does not inherit a previous import's ready state.

Validation: Java security-state and receipt tests; real Android keypad and security workflow instrumentation; synthetic session screenshot and equal-width/height/gap checks; 1024×600 head-unit layout measurement with a 480×280 minimum firmware area. Tests do not connect an adapter or call the security API. Pixel review build: preview.13; matching ARM32 build: headunit.7. Publication is separate from this device UI review.
