# Contributing to OpenSAAB T2

We welcome focused code changes, documentation improvements and careful adapter
reports. You do not need to understand the entire emulator to make a useful contribution.
Original OpenSAAB code is licensed under **Mozilla Public License 2.0 (MPL-2.0)**.
By submitting original contributions for inclusion, you agree to license them
under MPL-2.0. You retain your copyright; no copyright assignment is required.
Only contribute material you have the right to license, and preserve third-party
notices. See [LICENSE](LICENSE) and [licensing scope](LICENSING.md).
Public changes are reviewed through pull requests. Release source must match the corresponding build.

## Logo and branding

Use **OpenSAAB T2 — Open Connection**: the canonical master is
[opensaab-t2-logo-official.png](assets/branding/opensaab-t2-logo-official.png).
Follow [the logo policy and approved Android variant](assets/branding/README.md).
Older scanner/store/car-v2 concepts are historical only, not alternative release logos.

## Start small

- Reproduce an issue on a named app build and document exact steps.
- Improve one installation or troubleshooting explanation.
- Test a screen layout, accessibility behavior or failure recovery without a vehicle.
- Document an adapter's actual model, revision and USB identity; redact serials.
- Discuss protocol changes before attempting live tests or large rewrites.

Use Issues for reproducible bugs and compatibility reports. Use Discussions for
questions and proposals once the maintainer enables it. Check existing reports
before opening another. Do not publish VIN, plates, email addresses, passwords,
security seeds/keys, SSA blocks, firmware, proprietary DLLs or raw capture archives.
The app's support report is designed to exclude sensitive payloads; still review
it and your own description. DTC exports can contain vehicle identity.

## A pull request, step by step

A pull request (PR) asks maintainers to review a proposed change before merging it.

1. Discuss the issue first if it changes adapter behavior, firmware execution or API contracts.
2. Fork the public repository. Create a branch for one focused change.
3. Follow [Android Studio setup](ANDROID_STUDIO.md) or the Rust build instructions.
4. Make the smallest useful change and run the relevant tests. State whether tests
   were offline, simulated or on real hardware. A replay is not a fresh vehicle result.
5. Open a PR with the problem, resulting behavior, verification and remaining limits.
   Keep unrelated formatting and generated files out of the diff.
6. Respond to review; maintainers merge when the scope and evidence are clear.

Do not submit copied proprietary code or binaries. Identify third-party sources
and licenses when relevant. Do not remove a firmware assertion or invent an ECU
reply merely to make a test pass. Preserve original guest results and distinguish
host-generated discovery from original-firmware traffic.

## Useful checks

- Java/Android changes: `./gradlew :app:verifyDebugApk :app:lintDebug` with the documented JDK.
- Transport/request logic: `python3 scripts/android/test-request-gate.py`.
- UI changes: relevant emulator instrumentation suite described in ANDROID_STUDIO.md.
- Rust changes: relevant Cargo tests and formatting; run broader checks when the scope warrants them.

The build currently requires local proprietary support inputs and is not yet a
fully public reproducible build. Do not attach those inputs to an issue to make
CI pass. Release preparation must separate them or establish distribution rights.

## Working together

Be specific, patient and respectful. Critique code and evidence, not people.
No harassment, doxxing or posting private vehicle/account information. Maintainers
may redact reports, close abusive threads and restrict participation. This is a
volunteer project with no guaranteed response time. Never pressure someone to
perform vehicle programming to help reproduce an issue.
