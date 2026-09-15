# Install and update OpenSAAB T2 directly

Original OpenSAAB code is licensed under MPL-2.0; modified covered source must remain available to recipients when distributed. Separate third-party components retain their own terms.

Release: v0.1.0-preview.2, first public developer preview.

1. Start at https://www.opensaab.com/ or the official releases page: https://github.com/djfremen/OpenSAAB-T2/releases.
2. Download **OpenSAAB-T2-arm64-v8a.apk**. You need Android 8 or newer and a 64-bit ARM Android installation. Some car head units use a 32-bit Android system despite having a 64-bit processor; those are not supported by this APK.
3. Open the download. Android may ask you to allow installation from that browser or file manager. Allow that source, then return to the installer. You can turn that permission off afterward. Do not disable Play Protect.
4. Open OpenSAAB. Tap **Get started**, choose the diagnostic software, and download it. **English Saab NAO 9.250** is the default. Setup extracts and checks the file before activating it. A failed or cancelled download can be retried.
5. Connect a supported adapter through a USB data/OTG connection. Select it and accept Android's USB permission prompt. Choosing **Run in emulation mode** opens firmware without a vehicle connection.

The APK includes the three support files `eprom.bin`, `opsys.dwn`, and `candi.bin`. The larger Saab diagnostic card is downloaded separately. The support files retain their original authorship; the OpenSAAB source license does not grant rights to them.

## Without internet

An installed card, direct USB diagnostics, and locally saved reports do not require an internet connection. Reading the VIN from the vehicle is local; online vehicle enrichment may be unavailable. Setup needs internet to download a card, but **More options** also accepts a supported local BIN or ZIP. Keep about 140 MB free for setup and working copies.

**Security access requires an internet connection to the OpenSAAB security-access API. It cannot be processed offline.** Keep internet available while the request is being processed. If the connection or service is unavailable, the app reports the failure and offers a retry; it does not invent an authorization or import an incomplete result.

Software downloads, update checks, online vehicle details, donation pages, and email delivery also need internet. Preparing or saving a report locally does not. A previous security-access result is not a promise of permanent offline access.

## Updates

Use **Check for updates** on the home screen after finishing the vehicle session. Choose whether to include community previews. The app opens the official release page; Android asks before installation. Install the new APK over the current release to retain app data. Keep the same application ID and signing certificate across releases.

An older development/debug APK may have a different signature. **Do not uninstall it just to force an update:** that removes its private data. Export reports and firmware/security working data using supported app tools, verify the backup, then follow a separate migration procedure. Existing development installations must be backed up before migration; the public APK uses a different signing identity.

For users who already use Obtainium, the intended source URL is the official GitHub repository above. Its integration has not been tested with a published OpenSAAB release yet. Stable and preview release links will be added to the website once the corresponding files exist. A QR code should point to the website's installation page, not an expiring APK URL.

## Verify and get help

Each release should include its source tag, `SHA256SUMS`, `SIGNING-CERTIFICATE.txt`, and a build receipt. A checksum detects a changed download; the Android signing identity is what allows updates from the same publisher. The initial local candidate is not yet a claim of independently reproducible builds.

Use **Report issue** for connection problems and **DTC reports** for saved code reports. Review the report before sharing it, especially its VIN. Report issues at https://github.com/djfremen/OpenSAAB-T2/issues. Donations are optional: https://ko-fi.com/djfremen.
