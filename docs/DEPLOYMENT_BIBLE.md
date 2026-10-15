# The OpenSAAB Deployment Bible

## The ten commandments

Adopted October 14, 2026 · Revision 2. Inspired by the [ArtCraft workflow review](ARTCRAFT_WORKFLOW_REVIEW_2026-10-14.md), adapted to OpenSAAB's original-firmware workflows, independent platform channels and exact-artifact evidence. These are OpenSAAB's rules, not a reproduction of ArtCraft's private standards.

Apply these commandments to application releases, installers, update catalogs, repository documentation and website deployments. The existing [product standard](../AGENTS.md), [interface contract](../emulator/ui/interface-contract.json) and [qualification procedure](../releases/consistent-baseline/README.md) supply the detailed requirements. The [delivery playbook](OPENSAAB_DELIVERY_PLAYBOOK.md) describes the proposed implementation. Record checks manually where automation is not yet implemented; this document does not establish that a pipeline already enforces them.

### I. Thou shalt ship one OpenSAAB

Use the same setup, controls, labels, firmware key meanings and adapter behavior across supported platforms. Follow the latest qualified Android reference and implement shared behavior once with thin OS bindings. Record incomplete targets and deliberate contract changes in the common matrix.

**Evidence:** the interface contract revision, affected targets and conformance results. An incomplete platform stays an explicitly scoped preview.

### II. Thou shalt know thy source

Build release candidates from a clean, reviewed source snapshot with a full commit SHA. Keep changes focused and preserve unrelated ongoing work. Every distributed executable must have its exact matching source and retained build inputs, including applicable third-party notices.

**Evidence:** source/core revisions, matching source archive, dependency pins and review record. Never infer provenance from a filename or the current working directory.

### III. Thou shalt give every attempt its true name

Assign a distinct full version to every changed executable or adapter-capability attempt, including unpublished experiments. Include that version, its suffix/revision and architecture in the installed application name, program/Dock name, main window title, installer and manifest. Read displayed identity from packaged metadata.

**Evidence:** actual installed identity and SHA-256 recorded before testing. Historical example: `OpenSAAB-0.2.23-ui-parity.3-macos-x86_64.app`, displayed as `OpenSAAB 0.2.23-ui-parity.3 Intel`. Renaming an older binary does not create a new capability.

### IV. Thou shalt automate the repeatable work

Reuse build, packaging, signing, checksum and draft-release logic. Pin release workflows, toolchains and dependencies to reviewed identities. Prepare the candidate once and promote those exact bytes; a changed rebuild receives a new version and its own evidence.

**Evidence:** build recipe, workflow/toolchain pins, logs, package manifest, hashes and signing/notarization status. Keep quick development checks efficient without dropping required release checks.

### V. Thou shalt prove the artifact thou dost ship

Exercise the actual installed candidate on the environment for which a claim is made. Check relevant behavior, visible UI, failure handling and cleanup; use live hardware when the claim depends on it. Keep source checks, simulations, helper probes and physical results distinct.

**Evidence:** exact package hash, OS/architecture, adapter model/firmware, scenario, result and retained receipt. A pass from another binary, adapter or architecture cannot qualify this one. Pending work stays visible in preview notes; uniform-release claims require the common qualification gate.

**Mandatory security-access check:** every version and platform requires the
case-sensitive password `OpenSAAB` before security-access processing or
processed-response import. Verify acceptance and rejection paths on the exact
installed package, including automation, remembered credentials and fallback.
A target with no implementation must prove access remains unavailable. Missing
or pending password evidence blocks publication even for a scoped preview.

### VI. Thou shalt preserve the user's work

Verify the installation and upgrade paths relevant to the release, preserving firmware, settings and reports. Protect ongoing diagnostic sessions. Keep private VINs, security material and raw captures within their established boundaries, and retain user review/consent for sharing reports.

**Evidence:** install/upgrade results, migration or recovery instructions, and relevant privacy checks. Never replace or rename an application during an active diagnostic session.

### VII. Thou shalt document the change with the change

Update affected instructions, known gaps, capability records and release notes alongside implementation. Tell the reader what changed, how to install it, what passed and what remains pending. Keep the README a clear front door and put historical detail in linked evidence pages.

**Evidence:** a newcomer can find the correct download, requirements, quick start, source, test scope and support route. Documentation must describe the packaged release rather than an unreleased working tree.

### VIII. Thou shalt publish from one release record

Select recommended releases explicitly by product, platform, architecture and channel. Generate or reconcile the README, homepage, download page, updater and release notes against that selection. Preserve existing catalog schemas and keep experimental channels separate.

**Evidence:** visible versions, actual download URLs, package hashes and matching-source links agree for the same selected channel. A newer experimental release need not replace the recommended build; each must be labeled clearly.

### IX. Thou shalt show what works and claim what is proven

Use real OpenSAAB screenshots and short workflow demonstrations with clear download and community actions. Caption media with its version, platform and scope; identify offline demonstrations and live vehicle results accurately. Reuse reviewed media and concise product facts across repositories, the website and authorized announcements.

**Evidence:** every capability claim traces to an appropriate result, and media has a known source and publication permission. Preserve OpenSAAB's identity, licenses, attribution and acknowledgments. Keep the maintained forum draft separate from the main website.

### X. Thou shalt verify the deployment and keep a way back

Prepare a concrete release draft and deploy within the user's authorization. Then inspect the public result: redirects, visible versions, real download targets, catalog/updater behavior and downloaded hashes. For website overlays, use the currently active backend image's immutable digest as required by the site procedure.

**Evidence:** a deployment receipt records the source/rendered identity, published artifact hashes, destination, verification time, observed result and rollback target. Preserve historical packages, tags and receipts. Roll back a recommendation or deployment reference; never overwrite a published signed bundle or repoint its historical tag.

## The release checklist

Complete this record for each deployment. Link to existing receipts rather than duplicating private logs. Mark a step not applicable only with a reason. A scoped preview may retain explicitly pending physical checks; it may not claim those checks passed.

```text
Release or deployment:
Product / full version / platform / architecture / channel:
Declared capabilities and remaining gaps:
Source SHA / shared-core and contract pins:
Package or site digest / matching-source reference:
Destination / selected catalog entry:
Previous verified deployment or recommendation:

[ ] I    Product contract and affected-target scope recorded.
[ ] II   Clean reviewed source and matching inputs retained.
[ ] III  Full packaged and installed identity verified where applicable.
[ ] IV   Build/process pins, hashes and signing status retained.
[ ] V    Exact-candidate checks and pending physical scope recorded; OpenSAAB
         security-access password gate (or unavailable feature) verified.
[ ] VI   Applicable installation, upgrade, data and session checks completed.
[ ] VII  Instructions, gaps, release notes and support links updated.
[ ] VIII README, website, downloads and updater agree for the selected channel.
[ ] IX   Public claims and demonstration media match their evidence.
[ ] X    Live deployment verified; receipt and rollback target retained.

Unresolved items and their effect on this deployment:
Verification time / reviewer / evidence references:
Outcome: scoped preview | qualified release | website/docs only | not deployed
```

A release is complete when the intended artifact is available, its public presentation agrees with it, and the deployment receipt is retained. A correct local edit or successful upload alone does not establish completion.

## Existing verification commands

The operational [release enforcement guide](RELEASE_ENFORCEMENT.md) defines the
required deployment record, gated publisher and completion receipt. Use those
entry points for publication; the commands below remain supporting checks.

From the canonical OpenSAAB repository, check inventory and matrix freshness:

```sh
python3 tools/release/baseline.py --check-matrix
```

For a candidate claiming the complete consistent baseline, also run the gate's regression tests and qualify its artifact/evidence directory:

```sh
python3 -m unittest discover -s tools/release -p 'test_*.py'
python3 tools/release/baseline.py --status /absolute/path/to/candidate.json --qualify --artifacts /absolute/path/to/artifacts
```

The paths above are placeholders. The qualifier checks recorded provenance and evidence bindings; it does not perform live vehicle tests or publish anything. Do not use a passing inventory check as a qualification claim.

From `/Users/mini4/Documents/Projects/opensaab-site`, validate rendered website links before deployment:

```sh
python3 build.py --check
```

Follow that repository's deployment procedure and verify the live pages afterward. None of these commands replaces the checklist's installed-artifact or publication checks.
