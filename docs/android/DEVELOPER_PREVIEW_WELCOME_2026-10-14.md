# Developer Preview launch screen — October 14, 2026

Private Android ARM64 candidate `OpenSAAB 0.1.0-findings.9 ARM64`, versionCode
`100080`, adds the owner's requested screen on every fresh app launch.

Before the workspace, firmware setup, updates or diagnostic startup, it shows
Developer Preview, the full installed version and architecture, the packaged
issue/release date, and an invitation to help development through Report issue.
Continue opens the common OpenSAAB workspace. Report issue opens the existing
report form; returning keeps the welcome visible until Continue. Reports retain
their normal preparation, review and explicit sending steps.

There is no permanent dismissal preference. The current launch's Continue state
survives activity recreation/rotation. Returning from reporting or resuming an
active session does not introduce another launch notice. A fresh launch repeats it.

The shared interface contract records the same copy, metadata semantics, actions
and lifecycle for all targets. Other target bindings/installed tests remain
pending; synchronizing their contract source does not rebuild their apps.

## Date metadata and future posting

`assets/build.json` contains `release_date` in ISO calendar form and
`release_channel=developer_preview`. The displayed version is verified against
the installed package. A missing or inconsistent date remains unavailable.
No installation, current runtime date or cached updater response supplies it.

This private candidate says **Private preview issued October 14, 2026**.
Published packages say **Released**, using their actual publication date.
The SDK-only public builder requires `--release-date YYYY-MM-DD` (or
`OPENSAAB_RELEASE_DATE`) when `--release` is selected. Invalid calendar dates
are rejected. Gradle pins the date alongside the version and code; change all
three deliberately for each new attempt, using `-PappReleaseDate=YYYY-MM-DD`
when preparing another dated preview. A rebuild must not change the same
version's original issue date. Do not describe an offline installed version as
the globally latest release.

## Installed evidence

The exact signed APK was installed on Android API 36 ARM64 `emulator-5554`.
Installed version/code/architecture and pulled APK bytes match the packaged
artifact. Packaging checks and Android Lint pass.

The installed welcome test verifies the version/date/private label, no workspace
or diagnostic startup before Continue, the actual Report issue form and return,
Continue to the app, activity recreation without another notice, and a new launch
showing it again. `preview-welcome.png` captures the actual installed view.
The native offline firmware test also passes through the new Continue gate,
then genuine Saab 148.000 Main Menu/F0/arrows/EXIT and clean Stop.

Artifacts, metadata, matching source and test receipts are retained at
`/Users/mini4/.local/share/opensaab/android-preview-welcome-20261014/0.1.0-findings.9/`.
The earlier `.7` package remains preserved. The `.8` intermediate package is
retained as unqualified after a console refresh restart fix; `.9` is the tested
candidate. No public package, updater listing or physical phone was changed.

Physical-device and other-target qualification, wide/large-font UI evidence and
the broader common release gate remain pending. Existing kernel, adapter,
capture, Data Display and saved-report behavior is retained.
