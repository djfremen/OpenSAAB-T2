# OpenSAAB 0.2.28-mxplus.4 — macOS Intel and Apple silicon

This private preview combines the MX+ native session binding with localized
security transfer recognition and off-by-default MCP. The same compiled native
emulator now supplies Chipsoft, Nano, MDI and MX+ backends. MX+ retains original
firmware read diagnostics and bounded key-status behavior; code clearing,
security collection and programming remain unavailable on the MX+ route.
Existing USB modes and legacy Carista EVO / vLinker behavior retain their scope.

The localized observer recognizes the session card's own TIS2000 instructions,
including Dutch's blank separator. It preserves English recognition, cache schema
3, raw LCD capture, cancellation and explicit processing consent. Generated card
fixtures are not proof of a completed vehicle collection. MCP remains off until
explicitly enabled in App menu or with `--mcp-dir`.

The matching iOS integration uses the same MX+ protocol, localized prompt source
and Apple byte bridge, with an External Accessory native transport and embedded
Rust guest. Device/app authorization and the actual vendor protocol are required
for a phone connection; a core build or simulator check is not physical support.
No vendor protocol is guessed or borrowed from another application.

Every package carries full version/architecture metadata and a separate archive
of exact native components and hashes. Previous signed apps remain unchanged.
This is a new candidate: exact installed tests and live qualification must be
recorded separately for each platform and adapter. No public release is implied.

## Build and test

Use macOS, Xcode command-line tools, Python 3, Rust and Cargo. Install the target
with `rustup target add x86_64-apple-darwin`. Use separately authorized support
files with the sizes/hashes in emulator/ui/support-files.json:

```sh
python3 emulator/installer/macos/build.py --arch x86_64 \
  --version 0.2.28-mxplus.4 --support-root /path/to/support \
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

The combined core also adopts the reference UART activity/timeout, TPU arbitration and switch-routing fixes. Recovered pSOS/file/display services from the broader private core are outside this scoped integration.
