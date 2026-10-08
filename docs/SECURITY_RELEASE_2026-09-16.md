# Security-access release candidates

- ARM64: 0.1.0-preview.12 (100012), source dbd3d2fb9cb8. Pixel 7 / Chipsoft / 2004 NG 9-3 live test passed through a rear parking-sensor module re-add reported by the owner.
- ARM32: 0.1.0-headunit.6 (100006), same shared source. Rebuilt native ARMv7 executables, automated/profile checks and pinned release signer passed. Live head-unit test remains pending; retain the experimental label.
- Setup: keep 0.3.3. It reads the per-architecture catalog over HTTPS and checks APK identity, size, hash and signer.

Publish signed APK assets and matching source tags before deploying service/catalog/releases.json. Preserve canonical updater asset names alongside the dated public aliases and checksums. Install updates over existing packages; do not uninstall or clear data. Website deployment updates only the static catalog/download pages, with the running API image as its immutable base.

Publication audit found no known local credentials, private-key/token markers, or nonfixture Saab VINs in newly published Git blobs and APK contents. Detailed test captures and security receipts remain outside this repository.

## Active UI and automation update

Published ARM64 `0.1.0-preview.18` (100018) and experimental ARM32 `0.1.0-headunit.12` (100012), both built from `5831108`. Source tags and canonical APKs, dated aliases and SHA256 sidecars are public. Downloaded canonical release assets match their recorded hashes and sizes. New source blobs and unpacked APK members passed the publication scan after identifying a synthetic instrumentation VIN fixture; private success evidence remains outside the repository.

Setup remains 0.3.3 and its live catalog selects these two versions. The website home/download release notes were updated. Deployment changes only static pages/catalog on top of the previously running immutable service image; runtime configuration is unchanged. Public catalog, page contents, Setup download and password-required API health verified after deployment.

Validation includes preview.18 installed in place on Pixel 7, shared menu/security tests and Android security instrumentation. The owner reported successful manual security collection on preview.17 with its processing/import receipt captured privately. The final splash-navigation fix still needs connected-vehicle retesting; this ARM32 revision still needs physical head-unit testing. Both releases retain preview/experimental status.
