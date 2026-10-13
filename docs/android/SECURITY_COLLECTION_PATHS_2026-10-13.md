# Explicit manual and guided fresh security collection

Owner report on Pixel 7 / public preview.64 / VCX Nano: Collect fresh data
restarted original firmware but produced no seed requests or visible progress.
This scoped Android fix starts from preview.66 source
`92c72fd5545efe008020e7f192aff7242c5976a8` and preserves its native payloads.

Get security access is reachable from Ready and adapter Actions. Collect fresh
data offers Manual collection and Guided collection. Manual leaves every firmware
key to the owner. Guided selects recognized original menus for an explicitly
confirmed Saab 9-3 Sport (9440), model years 2003–2012, with the year from the
fresh vehicle VIN. Assistance requires recognized English firmware; other
vehicles/languages use manual navigation. Original firmware owns seed requests.
No physical ignition prompt is acknowledged automatically. An operator firmware
key cancels assistance. Collection is local; processing/password/disclosure
consent occur only after completed fresh collection.

Fixes: remove Nano's unconditional navigator cancellation; prevent an older
matching imported receipt from overriding requested collection; preserve the
explicit choice and confirmed menu target through adapter restarts; expose the
same choice from Chipsoft/Nano/MDI Actions. Fresh sessions load a disposable card
with exactly the 714-byte SSA erased, retaining the original selected card and
receipt. Keep source-before separately from guest-before so fresh flash proof
and later changed-source/import validation use their respective baselines.
MDI gets a private support-file directory for the same disposable card.

Tests use an isolated AVD package with synthetic data, no vehicle, USB, API or
real firmware. Existing host navigator/protocol guards cover recognized F6
Get Security Access selection and allowed seed requests, and reject security
key submission in collection mode. New instrumentation checks old POST-AUTH,
manual/guided choice, local VIN identity without lookup, physical prompts,
manual takeover, exact SSA-only clearing/source preservation and processing
validation against the preserved original source. Actual seed responses and
end-to-end processing/import on the car remain separate installed-artifact tests.

Urgent Android oracle fix: shared Rust migration and Android shared-core cutover
remain tracked work. Android ARM32, Windows, Intel/ARM macOS and Linux receive the
common contract requirement but do not inherit implementation or test claims.
Linux is excluded from the owner's October 13 vehicle campaign.

Private candidate: 0.1.0-preview.67-security.1 / Android ARM64. Versioned launcher,
program/window labels and packaged metadata identify the exact build. Historical
public64/public66 APKs and native payload hashes remain preserved. No public
release or updater change is authorized by this local fix.
