# Android feature matrix — September 2026

Setup and the emulator packages have independent versions. An installer update does not add features to an already published emulator APK. This matrix describes release contents; physical vehicle compatibility requires separate testing.

| Feature | ARM64 preview.2 | ARM64 preview.3 (release candidate) | ARM32 headunit.3 | Setup 0.3.2 |
| --- | --- | --- | --- | --- |
| Direct private internet support upload; receipt/retry | No | Yes | Yes | Not an emulator |
| Copy support text / save ZIP | No | Yes | Yes | System report copy |
| Main system check / first-run check | No | Yes | Yes | Yes |
| Logo on welcome/firmware setup | Text only | Yes | Yes | Yes |
| Native ELF-based architecture label | No | Yes | No | Routes by Android ABI support |
| Build/ABI and console-event support summaries | No | Yes | Yes | Not an emulator |
| Bounded native CPU/RAM/frame timing; report-time resource context | No | Yes, new sessions | No | Static device check |
| Head-unit landscape display/tool column | No | No; phone layout retained | Yes, landscape >=720dp | Adaptive installer layout |
| VIN, DTC reports, donations, firmware library | Yes | Yes | Yes | Opens emulator |

Native modules are compiled separately. ARM64 remains com.opensaab.tech2; ARM32 remains com.opensaab.tech2.headunit32. Releasing preview.3 must not replace the ARM32 artifact or catalog entry. The existing approved support firmware assets and Saab-card download flow are unchanged.

Release checks: source/ABI/signature/hash; full Rust and Java checks; support API compatibility; release APK UI and offline native startup; report redaction/export/errors; actual private upload and receipt; in-place Pixel update preserving installed data; website/catalog retrieval. Hardware/vehicle diagnostics are a separate validation item, not implied by these tests.
