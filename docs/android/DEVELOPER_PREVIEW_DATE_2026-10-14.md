# Developer Preview version date — October 14, 2026

Android ARM64 `OpenSAAB 0.1.0-findings.13 ARM64`, versionCode `100084`, removes
the confusing Private preview issued wording from the startup notice. The screen
uses Developer Preview, the full installed version/architecture, and **Version
date: October 14, 2026**. That identity applies to the APK used for owner testing
and subsequently posted for development feedback.

The date remains pinned to `release_date` in the APK's `assets/build.json` and
validated against its installed version. Packaging source-state bookkeeping does
not change the user-facing channel. A new executable attempt requires a new full
version. The screen appears on every fresh launch, with Continue and Report issue;
rotation, returning from reporting and resuming an active session preserve state.

The APK retains the existing AI control (MCP) endpoint and Chipsoft/Nano bindings,
and adds the missing MDI binding. Its matching preview.70 MDI worker now uses the
same original LCD menu decoder, semantic revision and input-consumption sequence.
The carrier/protocol/command policy and bounded capture behavior remain unchanged.
MDI current liveness comes from the actual worker PID and matching output path;
its session identity uses the unique run rather than the shared native folder name. Enable it through App menu / AI control (MCP) in
the active adapter screen, then pair the local Android MCP bridge with that phone.
Native input admission still requires the current session, semantic menu revision
and manual input generation. Reports remain user-reviewed and explicitly sent.

The build is prepared for the owner's Chipsoft, MDI and Nano physical testing
before posting. Exact installed software evidence is recorded separately from
those physical results. No public listing or historical signed package is changed
by this preparation. The shared interface contract records the same labels/date
semantics for all targets; their native bindings and qualification remain pending.

Artifacts and matching source are retained at
`/Users/mini4/.local/share/opensaab/android-preview-welcome-20261014/0.1.0-findings.13/`.
The `.9` APK and its installed evidence remain preserved.

The APK uses normal firmware setup and bundles no OEM ROM files. SDK-only
packaging must use the new matching MDI bridge from
`core-mdi/mdi-firmware-engine/OpenSAAB-T2/target/aarch64-linux-android/release/tech2-emu`
with the retained USB module. Gradle builds and pins that bridge automatically.

The retained MDI startup harness also adopts the existing typed Saab 148.000
welcome detection. It accepts localized ENTER/version text only with the selected
card manufacturer record and active LCD. NativeManual uses canonical ENTER 16
before handing navigation to the operator; scripted targets retain their existing
bootstrap key. The installed MDI software test exercises this exact worker and
activity with an offline carrier fixture, independently of physical USB results.

## Exact installed candidate evidence

The signed `0.1.0-findings.13` / `100084` APK was installed on API36 ARM64
`emulator-5554`; pulled installed bytes, version and full application label match.
APK validation and Android Lint pass. The actual startup notice/report-return,
Continue/recreation/fresh-launch, recovered native firmware boot/navigation/Stop,
authenticated MCP endpoint, genuine offline MCP menus and MDI activity/worker
offline fixture tests all pass. MCP commands deliver the packaged encoder
directly. No report was submitted and no physical USB/vehicle command was sent.

This is the candidate for owner Chipsoft, MDI and Nano tests before posting.
Physical results, ARM32/other platforms and wide/large-font layouts remain
pending. `.10`, `.11` and `.12` intermediates and failed test evidence are retained
as unqualified attempts; `.9` remains the earlier qualified startup checkpoint.
