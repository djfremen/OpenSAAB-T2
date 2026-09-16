# Internet-only support reports — 2026-09-15

Head-unit preview 0.1.0-headunit.3 adds **Report issue → Prepare report → Send to OpenSAAB**. No Bluetooth, mail app, GitHub account or login is needed. Internet is required for sending. Users review the exact JSON first, including their own description. Nothing uploads automatically.

A successful upload returns an OS- report number after storage verification. The number is saved locally and can be copied. A failed upload keeps the local ZIP and offers retry, copy text, save ZIP or Android sharing. Retrying the frozen report keeps the same report number. Other options preserves sharing/email for devices with those apps.

## Privacy and storage

Only the bounded support summary is uploaded: app/device versions, architecture, summarized adapter events and available process/error metadata. Raw logs, bus payloads, VIN, SSA/security responses, credentials, firmware and screenshots are excluded by the report builder. The user's description is included and must be reviewed for private information. Server field validation is an allowlist, not a guarantee that user-entered descriptions contain no personal information.

Storage reuses the collector's existing R2 credentials on the server, in a newly created separate bucket `opensaab-support-reports`, with no public domain or public development URL enabled. New R2 buckets are private by default: https://developers.cloudflare.com/r2/buckets/create-buckets/ . Collector bucket `opensaab-capture` is unchanged. No credentials are included in the APK or repository.

Reports remain until an administrator deletes them; there is no automatic expiry or delivery-to-GitHub feature. OpenSAAB administrators can access reports for support. The hosting provider may process normal HTTP request metadata. The stored report does not add an IP address. Users can request deletion by providing the report number through the project's support channel.

## Maintenance paths

- UI: `android/shared/com/opensaab/usb/SupportReportActivity.java`
- Summary: `android/shared/com/opensaab/usb/SupportReports.java`
- HTTPS client: `android/shared/com/opensaab/usb/SupportUpload.java`
- API: `service/support_reports_api.py`
- Existing-server integration: `service/support_entrypoint.py`
- Deployment overlay: `service/Dockerfile.support`
- Tests: `service/tests/test_support_reports.py`, `android/tests/SupportReportInstrumentedTest.java`

POST `/api/support/reports` requires JSON, an explicit consent header and at most 64 KiB. The app uses HTTPS with normal certificate validation, bounded timeouts and no redirect following. Only a verified HTTP 201 receipt is success.

R2 objects: `support-reports/v1/OS-<24 hex>.json`. IDs derive from SHA256 of canonical JSON. Authenticated GET `/api/admin/support/reports/{report_id}` requires the existing administrator token in an Authorization Bearer header. Public report retrieval is not provided. `OPENSAAB_SUPPORT_BUCKET` can override the support bucket; it must remain private. The existing collector S3 configuration and admin token stay in deployment environment variables.

Limits are 10 requests per observed client address per hour and 100 globally per hour, held in process memory. These reset on restart and are not distributed abuse protection. A proxy may group clients together. Reports fail closed if storage is unavailable; there is no ephemeral server disk fallback.

## Validation and limits

Server tests cover storage roundtrip, admin-only access, consent, size/schema limits, rate limiting, stable retry IDs and storage failure. Android tests cover request/receipt handling, redirects/errors, safe summaries and export UI. Physical ARM32 performance and vehicle communication remain separate work. This release does not claim to fix slow emulator startup. The published ARM64 APK is unchanged.
