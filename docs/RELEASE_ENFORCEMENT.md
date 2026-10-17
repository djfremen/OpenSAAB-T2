# Enforcing the OpenSAAB deployment commandments

The [Deployment Bible](DEPLOYMENT_BIBLE.md) is mandatory through repository instructions and the release entry points. Every candidate carries a private evidence record. The publisher rejects an incomplete record before creating or publishing a GitHub release. Website deployment checks its own record before Docker/Koyeb writes. Completion requires a second check after verifying the public result.

The gate verifies hashes, recorded identities, clean source and declared evidence. Reviewers remain responsible for the truth of hardware results, visual observations, licensing and public claims. A checked box without its retained evidence is rejected.

## The standard release path

From the canonical OpenSAAB repository:

1. Prepare a clean reviewed source checkout and a candidate bundle outside it. Keep the executable, exact matching source, public package manifest, release notes and private evidence together with their hashes. The exact source commit must already exist in the destination release repository.
2. Create and complete the deployment record. Name each platform/architecture, actual installed identity, capabilities, test limits and publication surface.
3. Run the prepublication gate. Fix a failed check or retain an explicitly allowed preview limitation; do not switch to raw publishing commands to avoid it.
4. Run the gated publisher to create a verified draft. Inspect the concrete result, then publish within the owner's authorization using the same entry point.
5. Reconcile and inspect the selected channel's README, homepage, downloads, updater and release notes. Retain the live observations, finish the deployment check and run the completion gate with a new receipt.

```sh
python3 tools/release/deployment.py --init /private/candidate/deployment.json

python3 tools/release/deployment.py \
  --record /private/candidate/deployment.json \
  --bundle /private/candidate \
  --source-root /path/to/clean/source --phase prepare

python3 tools/release/publish.py \
  --record /private/candidate/deployment.json \
  --bundle /private/candidate \
  --source-root /path/to/clean/source \
  --receipt /private/candidate/draft-receipt.json
```

The initializer deliberately produces an incomplete record. It is a starting point, not a passing receipt. To publish the reviewed draft, repeat the publisher command with `--publish` and a new receipt filename. The default only prepares a draft. Existing published releases are rejected; their assets are never overwritten. Interrupted draft uploads can resume only with the same candidate identity and already-uploaded bytes, adding missing assets without clobbering.

GitHub can retain an untagged draft while its release-by-tag endpoint returns 404. The canonical publisher can locate that exact tag through the authenticated paginated release listing; it rejects ambiguous matches and verifies the recorded full source target, tag, notes fingerprint and existing bytes before upload or promotion. An uncertain attempt retains its receipt; retry with a fresh receipt and identical candidate assets rather than creating another draft or bypassing the gate.

After live verification, update the private record's deployment evidence and run:

```sh
python3 tools/release/deployment.py \
  --record /private/candidate/deployment.json \
  --bundle /private/candidate \
  --source-root /path/to/clean/source --phase complete \
  --receipt /private/candidate/completion-receipt.json
```

All paths above are placeholders. The publisher saves the original input as a `.record.json` snapshot beside its receipt, including for blocked or uncertain attempts. Retain the prepublication and completion versions of the record alongside their receipts. Keep evidence and receipts outside the clean source checkout. A publication receipt explicitly remains `published_pending_live_surface_verification` until the separate completion step succeeds.

## Record and manifest fields

`deployment.py --init` supplies the top-level fields and all ten check names. File references use a relative `path` inside the candidate bundle and its `sha256`. Public artifacts additionally specify their byte `size` and `role`: `package`, `source`, `manifest`, or `site`. Only the public artifact list and reviewed notes are uploaded by the GitHub publisher. Private check evidence is not uploaded.

For applications, add a target for every package:

```json
{
  "platform": "macos",
  "architecture": "x86_64",
  "artifact": "OpenSAAB-FULL_VERSION-macos-x86_64.dmg",
  "manifest": "OpenSAAB-FULL_VERSION-macos-x86_64-manifest.json",
  "capabilities": ["Explicitly described package capability"],
  "security_access": {
    "policy": "security-access-password-1",
    "mode": "no_operator_password"
  },
  "identity": {
    "artifact_sha256": "PACKAGE_SHA256",
    "version_source": "packaged_metadata",
    "installed_name": "OpenSAAB-FULL_VERSION-macos-x86_64.app",
    "program_name": "OpenSAAB FULL_VERSION Intel",
    "window_title": "OpenSAAB FULL_VERSION Intel",
    "installer_name": "OpenSAAB-FULL_VERSION-macos-x86_64.dmg",
    "manifest_name": "OpenSAAB-FULL_VERSION-macos-x86_64-manifest.json",
    "evidence": {"path": "private/installed-identity.json", "sha256": "EVIDENCE_SHA256"}
  }
}
```

Replace placeholders with observed values. Each public package manifest is JSON containing `product`, `version`, `source_revision`, `platform`, `architecture`, `artifact_sha256`, `capabilities` and `security_access`. The public `security_access` object contains only the target's `policy` and `mode`; private evidence is not uploaded. These fields must exactly match its target and package. The interface-contract file is included by hash and checked against the canonical contract.

The owner's dated Android ARM64 filename is
`OpenSAAB_64_DDMONYY_vMAJOR.MINOR.PATCH-preview.N.apk`, with the matching
`-manifest.json` filename. The identity gate recognizes this exact dated format
only for Android ARM64 installer/manifest names. Installed, program and window
names still carry `ARM64` or the full ABI token. Other targets and arbitrary
`64` labels do not receive this exception.

Each commandment check has `status`, a substantive result/scope `note`, and hashed `evidence` references. All ten are required. Before publication, deployment verification may be pending. A preview's artifact-test check may also be pending with retained narrower evidence and its exact limitation stated verbatim in the release notes. Other unresolved application checks block publication. A `qualified` channel must additionally pass the existing complete baseline qualifier; its `baseline_qualification.status` is a hashed reference to the candidate status JSON in the bundle.

Each of the five `surfaces` entries names `readme`, `homepage`, `downloads`, `updater` or `release_notes`, with an `action` of `update` or `unchanged`, a scope/reason `note`, and hashed evidence. An experiment may leave the recommended channel unchanged when this is explicit. Record actual visible versions and download targets in that evidence, then verify the live result after publication. Do not turn an unchanged recommendation into an inferred pass for the experiment.

The local identity ledger defaults to `~/.local/share/opensaab/deployment-ledger.json`. It reserves a product/version/platform/architecture against package bytes and capabilities. A machine-readable fingerprint in the remote draft's notes binds the prepared public assets and identities across CI runs, including interrupted uploads. Preserve both records. Administrators who delete or rewrite records can bypass process controls; these records are not a substitute for repository access restrictions.

## Security workflow access

Owner clarification October 16, 2026: implemented OpenSAAB features must not
require an operator password or feature entitlement on any platform or release
channel, including qualified releases. New packages declare
`mode: no_operator_password`; absent features declare `unavailable`.
No separate password-validation file is required for open access in either
channel. Consent, internal service authentication, cancellation, response
validation and SSA import integrity remain. The regular artifact-test and full
qualification requirements still govern their actual behavior.

The historical `preview_no_prompt` mode remains accepted for existing preview
records, including Windows 0.2.18-preview.5. Preserve published packages and
receipts. A fixed credential distributed in open source is not an operator
access-control boundary or a secret. Internal service credentials remain out of
fallback requests; this change does not alter backend authentication or consent.

## Historical password evidence

The following schema and cases describe retained `password_protected` records.
They are not a requirement to add a password to new packages. Compatibility
validation still checks the truth of a legacy protected record when supplied.
New open-access records take the common path above without this file.

A legacy target's hashed private `security_access.evidence` file is JSON with these
fields:

```json
{
  "policy": "security-access-password-1",
  "mode": "password_protected",
  "version": "FULL_VERSION",
  "source_revision": "FULL_SOURCE_SHA",
  "platform": "macos",
  "architecture": "x86_64",
  "artifact_sha256": "PACKAGE_SHA256",
  "password_sha256": "SHA256_OF_CASE_SENSITIVE_PASSWORD",
  "status": "pending",
  "reviewed_by": "",
  "observations": [],
  "cases": {}
}
```

This template intentionally fails. Replace placeholders with the installed
package's identity. `observations` must contain retained file references with
`path` and `sha256`, such as reviewed UI recordings and bounded test logs.
`password_sha256` must be the SHA-256 of the exact UTF-8 bytes of `OpenSAAB`,
without a newline. Do not record entered passwords in logs or screenshots.
Complete each case with a `result` and a substantive observation `note`:

| Case | Required result |
| --- | --- |
| `correct_password` | `accepted` |
| `empty_password` | `blocked` |
| `incorrect_password` | `blocked` |
| `case_changed_password` | `blocked` |
| `cancelled_password` | `blocked` |
| `fallback_without_password` | `blocked` |
| `invalid_remembered_password` | `blocked` |
| `import_without_password` | `blocked` |

If the installed candidate has no security-access processing/import feature,
use `mode: unavailable` in the target, public manifest and private proof. Replace
the case list with `all_security_entry_points`, result `unavailable`, and a note
describing the checked UI and automation routes. Retain observations proving the
feature cannot be invoked. A declaration of missing support is not itself proof.

Set `status: pass` only after reviewing those exact-package observations. The
validator binds the evidence and checks recorded outcomes; it does not operate
the installed app or manufacture runtime passes. All seven platform targets
remain pending installed verification in the baseline matrix. No historical
package has been altered or retroactively qualified. A changed executable or
security capability receives a new full version; the ledger also rejects a
changed security mode under a reserved version.

## Website entry point

Use `kind: website`, `channel: website`, an empty target list, and a retained ZIP of the rendered `dist/` contents at the ZIP root. Set `site_tree_sha256` with:

```sh
python3 tools/release/deployment.py --site-digest /path/to/clean/site/dist
```

The gate compares both ZIP contents and actual rendered files against that digest. It rejects unsafe archive paths and symlinks. Website-only installed-identity, application-test and user-data checks may be `not_applicable` with a reason; other requirements still apply.

Set `OPENSAAB_SITE_ROOT` to the clean reviewed site checkout, `OPENSAAB_DEPLOYMENT_RECORD` to its private record, and `OPENSAAB_DEPLOYMENT_BUNDLE` to the retained bundle. Continue setting the verified current `OPENSAAB_SITE_BASE_IMAGE` digest as required by the site README, then run its `deploy/koyeb-overlay.command`.

The command checks the actual source and copied deployment tree before remote writes, binds resumable image state to the record, and compares deployed bytes at both the service origin and `www.opensaab.com`. A legacy pending image without a matching binding must be inspected or rebuilt. Keep the final deployment record outside the site source and run the completion gate with `--site-root /path/to/clean/site/dist` after all selected-channel surfaces have been verified.

## CI and repository activation

Two workflow files are prepared locally:

- [Deployment standard](../.github/workflows/deployment-standard.yml) runs gate rejection-path regressions on PRs and main.
- [Gated application release](../.github/workflows/publish-release.yml) accepts a reviewed candidate artifact run/name and matching source repository/SHA. It checks out the source, tests the gate, downloads the bundle without executing its contents, and invokes the gated publisher. Its default is a draft. Publication receipts and original records remain in private run artifacts, with the receipt also in the run summary. Archive needed evidence before the run's configured artifact-retention period expires.

These workflows become available only after their reviewed changes reach GitHub. The publisher workflow also needs an explicitly configured `OPENSAAB_RELEASE_TOKEN` with the necessary destination-repository release permission. Keep publisher credentials confined to the release job/environment; do not distribute them to arbitrary build or PR jobs.

For stronger server enforcement, protect the publishing branch, require the applicable status checks, restrict the release environment to that branch, and restrict release-writing credentials to the gated publisher. GitHub permits collaborators with write access to create releases through other interfaces, so a CI file alone cannot make the process impossible to bypass. [GitHub release permissions](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository), [environment protection](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).

On October 14, 2026, GitHub's API rejected rulesets for the private OpenSAAB repository with a plan limitation; no rulesets or new credentials were configured during this implementation. Keep the repository private unless the owner deliberately chooses otherwise. Server-enforced activation requires an eligible plan/configuration or an intentionally chosen publishing repository with the needed protections. Until then, the local gated entry points and agent instructions enforce the normal path, while a sufficiently privileged direct publisher can still bypass them.

## Regression verification

```sh
python3 -m unittest discover -s tools/release -p 'test_deployment.py' -v
```

The tests use synthetic packages and mocked GitHub calls. They cover missing commandments, identity and manifest disagreement, changed evidence, path escapes, version reuse, dirty source, pending preview scope, completion separation, unsafe website archives, published-release protection and interrupted uploads. Security regressions retain legacy proof checks and verify that open feature access needs no password-validation file in either preview or qualified channels. They do not create a release or claim installed-app or hardware qualification.

The separate baseline suite continues to govern complete cross-platform qualification. The current local inventory/contract mismatch remains visible there; do not weaken its target set or checks to make an unrelated preview pass.
