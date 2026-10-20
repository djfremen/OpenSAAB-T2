# OpenSAAB 0.2.27-preview.4 — macOS Intel and Apple silicon

Scoped native macOS development build on published 0.2.27-preview.3. It recognizes the TIS2000 security transfer prompt on every diagnostic-card language: the prompt is read from the session card's own message database (title naming TIS2000, step 1, RS232 step) and matched against the LCD, so a non-English card can reach Process security data instead of looping at the transfer prompt. English keeps its original wording check. Prompt extraction is verified on the German 148.000 and English NAO 9.250 cards; the other 148.000 languages and live collections remain pending. The LCD capture also keeps a diagnostic `screen.raw` with the guest's original character codes. Local AI control (MCP) is included and stays off until enabled from the app menu.

Preview.3 behavior (Nano HS-CAN filter, bounded key-status reads, docked keypad, no operator password, consent and SSA-only import validation) is retained. This is a new build candidate, not a new physical qualification. Intel and Apple-silicon checks are recorded separately. Wider adapters/vehicles, sustained use and complete conformance remain pending. The historical [Intel scope](emulator/docs/INTEL_PREVIEW_2026-10-14.md) keeps its original version and evidence.
This clean source excludes private Git history, vehicle runs, credentials and
firmware/card binaries. A historical negative VIN fixture is artificial; its
integrity pin was regenerated before building. Project/upstream licenses remain.

## Build and test

Use macOS, Xcode command-line tools, Python 3, Rust and Cargo. Install the target
with `rustup target add x86_64-apple-darwin`. Use separately authorized support
files with the sizes/hashes in emulator/ui/support-files.json:

```sh
python3 emulator/installer/macos/build.py --arch x86_64 \
  --version 0.2.27-local.2 --support-root /path/to/support \
  --output /path/to/new-output
cargo test --locked --features load-test --manifest-path emulator/Cargo.toml
```

Choose a new full version for every changed build; preserve published artifacts.
`--without-support` creates an explicit import-only package. The diagnostic card
is selected/downloaded separately through the app. The replay runner requires
separately hash-verified original cards and never opens an adapter. Private test
operations require explicit local enablement and an isolated state directory.

See emulator/LICENSING.md and emulator/THIRD_PARTY_NOTICES.md. Independent binary
reproduction is unverified. Ad-hoc signing and toolchain/package metadata vary.
[Downloads](https://github.com/djfremen/OpenSAAB-T2/releases) ·
[Community](https://github.com/djfremen/OpenSAAB-T2/discussions/5).
