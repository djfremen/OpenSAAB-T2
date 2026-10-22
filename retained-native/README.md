# Approved native inputs for the MCP latency candidate

The MDI carrier, MDI firmware worker and connection worker are copied from the
exact installed/tested OpenSAAB 0.1.1-preview.1 ARM64 APK, SHA-256
`730bde2749430ecd49fbe309dc838342ec28c4a2e041c497e5480f251d44ece2`,
source commit `d18faaa3c59eace58b61983988abdd218067a1d6`.
This directory preserves historical inputs. Current unchanged Nano/Chipsoft
probe hashes remain pinned in
`android/tech2-app/native-payload-baseline.json`. The preview.33 build deliberately
rebuilds the main/MX, recovered and MDI firmware engines, MDI carrier and shared
connection worker. Historical MDI and connection files here are not substituted
for rebuilt components. Only the two Nano/Chipsoft probes remain retained, with
exact preview.1 source provenance and byte pins. See
`android/tech2-app/preview-build.json` and `docs/ANDROID_PREVIEW_BUILD.md` for the
complete current input policy.

Preview.5 inadvertently packaged the older retained MDI files from the source
tree instead of the preview.1 build inputs. That APK and its receipts remain
immutable historical evidence. Preview.6 restores the exact preview.1 payloads.
The command-line builder, Gradle staging and final APK checker verify full native
coverage and unchanged hashes. A deliberate native capability change requires
updating this reviewed baseline and using a new full version.

MDI notices remain in `android/adapters/mdi/MDI_NOTICES.txt`; no new vehicle or
protocol qualification follows from retaining these bytes. New Java/UI bindings
still need exact Fire/adapter validation.
