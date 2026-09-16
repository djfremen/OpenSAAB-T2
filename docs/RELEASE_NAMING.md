# OpenSAAB release names

Public APK names use **OpenSAAB_<Setup|32|64>_<DDMONYY>_v<versionName>.apk**.

- `Setup` is the universal Java-only installer; `32` means ARMv7, `64` means ARM64.
- Date is the original publication date in America/Los_Angeles. Use uppercase English month abbreviations with `SEPT` for September, e.g. `15SEPT26`.
- Version must equal the Android manifest's versionName, including preview/headunit suffixes. Do not relabel an older APK as a new version. A future genuine 0.5.1 ARM64 release could be `OpenSAAB_64_15SEPT26_v0.5.1.apk` if actually published that day.
- Keep Git tags, package names, signing keys and versionCode ordering unchanged. Architecture channels remain independently versioned.
- Preserve legacy APK assets permanently: installed Setup/updater versions may require their exact URLs. Upload readable byte-identical aliases; never remove or rename a legacy asset. Keep original source/build receipts and checksums immutable; add a `.apk.sha256` sidecar for the new alias.
- Website advertises only the readable Setup APK; architecture-specific aliases are for release inspection and developer testing.

## Packaging

After signing and verifying the official release APK, run:

```sh
python3 scripts/android/name-release-apk.py target/compatibility-checker/OpenSAAB-Setup.apk --release-date 2026-09-15 --output target/named-releases/setup-v0.3.1
```

The helper reads the actual package and version from the APK with Android `aapt`, checks packaged native ABIs, copies without changing bytes/signature, refuses conflicting existing output, and writes a checksum. It does not sign, change the app version, upload, or change catalog URLs. Upload alias and sidecar to the same GitHub release and use the basename (without `.apk`) as its release title. Continue uploading the legacy asset for old installed clients.

Setup release: `OpenSAAB_Setup_15SEPT26_v0.3.2.apk`. Previous emulator aliases: `OpenSAAB_32_15SEPT26_v0.1.0-headunit.3.apk`, `OpenSAAB_64_14SEPT26_v0.1.0-preview.2.apk`. ARM64 was published September 14 Pacific (September 15 UTC); its original date is preserved.
