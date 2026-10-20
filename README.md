# OpenSAAB 0.2.27-preview.5 — macOS Intel and Apple silicon

Scoped native macOS source candidate on 0.2.27-preview.4. The Dutch 148.000 card uses a blank separator in its TIS2000 transfer message; preview.4 only accepted dashes and extracted no prompt from that card. This revision accepts the card's blank separator and requires a matching blank LCD row, the complete card-derived title, step 1 and RS232 line in order. Other cards retain their dashed-separator matching, and English keeps its original wording check. Cache schema 3 forces a fresh extraction after upgrading from preview.4.

Extraction and matching passed on all ten catalog-verified runnable images: English NAO 9.250 and English, German, Spanish, Finnish, French, Italian, Dutch, Russian and Swedish 148.000. These are source tests using real card bytes and generated LCD layouts, not live collection proof.

The LCD capture retains diagnostic `screen.raw` bytes alongside ASCII `screen.txt`. Local AI control (MCP) stays off until enabled from the app menu. The Mac shell supplies the session folder to the shared observer; Android, Windows and iOS adoption of that observer argument remains pending. Exact preview.5 installed checks and live collections remain pending; preview.4 observations do not qualify this revision.

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
