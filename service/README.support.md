# Support API deployment source

`OpenSAAB-T2/service/support_reports_api.py` is the deployment source of truth. The canonical OpenSAAB product repository keeps a generated mirror in `service/support/`; run `sync_canonical_support.py` to update it and `--check` to detect drift. Its manifest pins the source commit and file hashes. Make API changes here, then regenerate the mirror. The mirror's test imports are adjusted for its nested Python package; the API module is byte-identical.

## Build without rolling back the live service

Inspect the active Koyeb deployment and use its exact `repository@sha256:...` image digest as `OPENSAAB_SUPPORT_BASE_IMAGE`. The Dockerfile deliberately has no default base. It replaces only `/app/support_reports_api.py` and inherits the base command, environment, security API, password handling, collector and website.

The October 16 live image starts `collector_entrypoint:app`; it has no `server` module. The old September 15 base and `support_entrypoint.py` command therefore do not describe the current deployment. That helper remains historical source and is not copied or selected by the Dockerfile. Do not restore its CMD or build a service update from the old mutable base tag.

From a clean reviewed checkout:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s service/tests -p 'test_*.py'
python3 service/sync_canonical_support.py --canonical-root /path/to/OpenSAAB
python3 service/sync_canonical_support.py --canonical-root /path/to/OpenSAAB --check
```

Build with `docker build --platform linux/amd64 --file service/Dockerfile.support --build-arg OPENSAAB_SUPPORT_BASE_IMAGE=EXACT_LIVE_DIGEST --build-arg OPENSAAB_SUPPORT_VERSION=DISTINCT_SERVER_VERSION --build-arg OPENSAAB_SUPPORT_SOURCE_REVISION=FULL_COMMIT --tag DISTINCT_IMAGE_TAG service`. The uppercase values are placeholders. Retain the resulting image digest and compare its inherited application files/command with the baseline before deployment. Do not copy production credentials into source, build arguments, image layers or logs.

## Report compatibility and privacy

Format 1 retains canonical receipt bytes. The known `normal_stop` reason is accepted for older iOS clients; arbitrary reason text is still rejected. Format 2 requires preparer and session-launch identities; the core may never have started. Panic locations and all three offline profiles remain accepted. Identity metadata goes to derived `support-index/v1/` and `support-submissions/v1/` objects; original reports stay under `support-reports/v1/`. A failed sidecar write does not reject a confirmed report. Old reports are not automatically backfilled by deploying this module.

Claims match a trusted release manifest only when its recorded hashes/version/revision agree. This is a metadata comparison, not remote attestation that a client actually ran those bytes. No trusted manifest is generated or published by this patch. Without one, claims remain explicitly unverified.

The operations brief excludes logs, stacks, descriptions and panic locations. The private fingerprint triage job must accept both stored report formats before format-2 clients ship; older `0.1.0-cloud.3` code rejected format 2. Its `0.1.0-cloud.4` compatibility update preserves grouping and keeps identity containers out of model prompts. Its recurring execution remains a separate deployment/capacity decision.

Keep admin routes private and authenticated; do not open the production admin surface to run backfill. Verify .92-shaped format-1/2 uploads, old receipt stability, iOS/desktop acceptance, malformed identity/panic rejection, service health and unchanged public website/security behavior before declaring a rollout complete. Retain synthetic test receipts separately from user incidents.
