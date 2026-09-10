# OpenSAAB T2

Original diagnostic menus on Android, powered by a Rust emulator and direct USB adapters.

**Developer preview:** the community is open. The clean source release and signed Android APK are being prepared; no public APK is available yet.

## Community

- [Questions and ideas](https://github.com/djfremen/OpenSAAB-T2/discussions)
- [Report a reproducible issue](https://github.com/djfremen/OpenSAAB-T2/issues/new)
- [Volunteer for Android testing](https://github.com/djfremen/OpenSAAB-T2/discussions/1)

For a useful issue, include the app build, phone model, Android version, adapter, menu path, expected result and actual error. Review any support report before attaching it. Never publish VINs, security seeds/keys, SSA files, raw USB captures, firmware, passwords or unredacted screenshots.

## Current status

| Area | Status |
| --- | --- |
| Chipsoft Pro on Android | Current development focus; ECU information, DTC and selected security workflows tested on project vehicles |
| VCX Nano on Android | Earlier successful runs; a later channel-initialization regression remains unresolved |
| MDI and other adapters | No supported Android backend yet |
| Live data and programming | Incomplete coverage; wider vehicle and module validation needed |

The emulator and original vehicle firmware are separate. Public-build packaging, firmware distribution and source licensing are being finalized. This repository currently hosts community information.

## Android beta testing

Testing is voluntary. Reply to the welcome discussion with your phone model, Android version and adapter; no email or VIN is needed. Each testing round will have a specific build and a short checklist. Start with information and code reading; do not change vehicle configuration solely to generate a report.

A Google Play closed test is not running yet. When available, it will use its own Google opt-in link. Joining this repository does not satisfy Google Play testing requirements.

Independent community project. Not affiliated with Saab, GM or adapter manufacturers.
