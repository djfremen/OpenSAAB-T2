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

## Security access password

Owner clarification October 16, 2026 supersedes the preview portion of the
October 14 requirement: developer previews have no operator security-password
prompt and require no separate security-validation evidence file for publication.
Keep service authentication internal and preserve consent, cancellation, response
validation, SSA import checks and features that are unavailable.

Qualified releases still require the case-sensitive password `OpenSAAB` before
security-access processing or processed-response import, including automation
and fallback routes. Follow `security_workflow.password_policy` in the shared
interface contract. Exact installed-package password evidence remains mandatory
for qualified releases. Preserve historical packages and their evidence.

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
