# Security-access release candidates

- ARM64: 0.1.0-preview.12 (100012), source dbd3d2fb9cb8. Pixel 7 / Chipsoft / 2004 NG 9-3 live test passed through a rear parking-sensor module re-add reported by the owner.
- ARM32: 0.1.0-headunit.6 (100006), same shared source. Rebuilt native ARMv7 executables, automated/profile checks and pinned release signer passed. Live head-unit test remains pending; retain the experimental label.
- Setup: keep 0.3.3. It reads the per-architecture catalog over HTTPS and checks APK identity, size, hash and signer.

Publish signed APK assets and matching source tags before deploying service/catalog/releases.json. Preserve canonical updater asset names alongside the dated public aliases and checksums. Install updates over existing packages; do not uninstall or clear data. Website deployment updates only the static catalog/download pages, with the running API image as its immutable base.

Publication audit found no known local credentials, private-key/token markers, or nonfixture Saab VINs in newly published Git blobs and APK contents. Detailed test captures and security receipts remain outside this repository.
