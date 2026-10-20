# OpenSAAB product standard

The owner's standing requirement is the “McDonald's fries” principle: the same
OpenSAAB interface, setup, controls, features and adapter behavior on every
supported platform. The latest qualified Android interface is the reference;
do not invent a different desktop product or treat an engine-only port as done.

- Read `emulator/ui/interface-contract.json` and `emulator/ui/CONFORMANCE.md`
  before changing UI, setup or platform workflows. Use the common labels,
  layout rules, firmware key semantics and state meanings.
- Keep platform and adapter transports separate internally; users should follow
  the same workflow and receive equivalent results. Preserve existing working
  implementations while migrating.
- Update the shared contract deliberately when the product changes and track
  the change on all declared targets in `releases/consistent-baseline/status.json`.
  Do not silently fork the interface per platform or architecture.
- A platform can remain an explicitly incomplete preview. Do not describe it as
  uniform or release-qualified until the exact installed artifact passes the
  common visual/workflow scenarios, including live hardware where required.
- Keep evidence and the personal dashboard current. No inferred passes from
  another architecture, another adapter, standalone helpers or an older binary.

These rules do not require extra user approval for already-authorized work.

## Deployment standard

Read and follow [`docs/DEPLOYMENT_BIBLE.md`](docs/DEPLOYMENT_BIBLE.md) before
preparing an application release, installer, update-catalog change, release
documentation or website deployment. Its ten commandments and release checklist
apply alongside the product and preview-identity requirements below. Record the
candidate's exact scope and evidence; do not infer automated enforcement from
the written standard. Continue already-authorized work without adding a new
approval step solely because this checklist exists.

Use `tools/release/publish.py` for GitHub application release drafts/publication;
do not bypass it with direct release-create, asset-replacement or publish calls.
Follow `docs/RELEASE_ENFORCEMENT.md`: retain a machine-readable deployment record,
run the prepublication gate, and run the completion gate with a new receipt after
verifying live public surfaces. A preview's pending physical checks must remain
explicit. Website deployment must use its gated deployment entry point. These
requirements govern already-authorized publication; they do not authorize a new
release, public disclosure, credential change or announcement by themselves.

## Security workflow access

Owner clarification October 16, 2026: OpenSAAB features must not require an
operator password or feature-access credential on any platform or release
channel, including qualified releases. This supersedes the October 14 password
requirement and the earlier preview-only exemption. A fixed credential included
in distributed open source is not a product access-control boundary.

Preserve internal service authentication, explicit consent, cancellation,
response validation and SSA import integrity checks. Unavailable features remain
unavailable. New bindings must use the common no-operator-password policy rather
than restoring a password dialog for qualified or unknown channel metadata.
Record each target's source adoption and exact installed verification separately;
policy adoption alone does not qualify an executable or a vehicle workflow.
Historical packages, manifests and evidence remain immutable.

## Development preview names

Owner requirement recorded October 12, 2026: every development attempt must
carry its full version in the installed program name. Include the experiment
or prerelease suffix, attempt revision and architecture; generic names such as
`OpenSAAB.app` or `OpenSAAB Intel Preview.app` are not acceptable for new builds
or deployments. Each changed executable/capability attempt needs a distinct
version. Do not reuse a version with changed program contents.

The installed application name, program/Dock name, main window title, installer
and manifest must agree with the packaged version. Example:
`OpenSAAB-0.2.23-ui-parity.3-macos-x86_64.app`, displayed as
`OpenSAAB 0.2.23-ui-parity.3 Intel`. Verify the installed identity and hashes
before testing. Preserve historical signed packages and evidence; do not rename
an app while its diagnostic session is running.

Owner clarification October 14, 2026: distributed ARM64 Android Developer Previews
retain `-preview.N`; labels such as `findings` belong to internal research and
historical debug attempts. Keep the full packaged version and architecture on
every identity surface, and retain historical packages without relabeling them.
For the current Android work, finish debugging first, then test Chipsoft, MDI
and Nano through the Android implementation and its MCP controls before posting.

## Migration discipline

Follow `releases/consistent-baseline/PLATFORM_CONVERGENCE_PLAN.md`. Before implementing
a desktop workflow, inspect the qualified Android implementation and its tests.
Port the complete call chain into shared logic with thin OS bindings; do not independently
recreate business rules in Swift, C# and Python. Preserve Android as the regression
reference until the shared replacement is qualified. Scoped fixes remain previews and
must record uncompleted platform migration/verification work in the matrix.

Shared adapter protocol behavior is documented in
[`shared/adapter-translation/SPEC.md`](shared/adapter-translation/SPEC.md).
Use its canonical frame/decoder contracts when extending adapter or application
workflows. It distinguishes completed source adoption from pending driver,
installed-artifact and physical qualification; current source pins are recorded
there and in the baseline status.

## Native components for every version and adapter

Owner requirement October 20, 2026: retain the native emulator/workflow and adapter
components for every application version, preview, architecture and hardware
adaptation. This includes Android `.so` payloads and the corresponding native
artifacts on other targets. Keep the exact component bytes with a manifest naming
the full application version, target/ABI, source revision, component SHA-256 hashes,
adapter capabilities and verification status. Identical components may be reused
only with explicit hash pins; a missing component remains an explicit gap.

Use each platform's actual binary format: Android/Linux ELF, Windows PE, macOS
Mach-O workers/libraries, and iOS embedded static libraries/frameworks. A renamed
`.so` does not establish an Apple port. Preserve historical packages and append
separate component archives/receipts rather than altering signed bundles.

The October 20 localized-security implementation must carry forward through the
shared source to Apple silicon and the iOS preview, as well as the other declared
targets and adapter bindings. Keep OS transports and thin host bindings explicit;
build, installed-app and physical-adapter verification remain separate per target.
An iOS core build alone does not establish the complete security UI/adapter workflow.
