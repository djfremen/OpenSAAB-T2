# OpenSAAB Intel 0.2.26-intel-preview.3

Scoped native macOS Intel development preview for diagnostic language activation
and deferred CANdi startup. Read [the release scope](emulator/docs/INTEL_PREVIEW_2026-10-14.md).
This clean source excludes private Git history, vehicle runs, credentials and
firmware/card binaries. A historical negative VIN fixture is artificial; its
integrity pin was regenerated before building. Project/upstream licenses remain.

## Build and test

Use macOS, Xcode command-line tools, Python 3, Rust and Cargo. Install the target
with `rustup target add x86_64-apple-darwin`. Use separately authorized support
files with the sizes/hashes in emulator/ui/support-files.json:

```sh
python3 emulator/installer/macos/build.py --arch x86_64 \
  --version 0.2.26-intel-local.1 --support-root /path/to/support \
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
