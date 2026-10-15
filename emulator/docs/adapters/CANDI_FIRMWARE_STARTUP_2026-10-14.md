# Saab firmware welcome and deferred CANdi startup

Candidate: `0.2.25-candi-startup.1`, built separately for macOS ARM64 and Intel.
Predecessor source: `843b402` (native MDI .3 lineage). Shared guest behavior lives
in `src/bus.rs`; the OS adapter bindings and command permissions are unchanged.

## Failure and change

The former welcome predicate required English `Software Version`, an English
ENTER prompt and `North American Operations`. Every catalog 148.000 card failed
this gate. CANdi stayed pending; communication entry could continue with no
initialized CANdi and subsequently halt in guest cleanup.

The 148.000 predicate now requires a T2 card with the typed Saab manufacturer
record, plus the version and the ENTER key token in the active LCD text layer.
Translations of the prompt/version label and ENTER punctuation no longer matter.
English 148.000 displays `Ignition Cycles:` in place of the manufacturer line;
Italian capitalizes the manufacturer. Neither changes the selected card identity.
Hidden text, card metadata alone, a different product or another version cannot
establish this welcome. The existing NAO welcome predicate remains supported.

This gate only arms deferred initialization. The first original guest dependency
still uses the existing CANdi loader code/stack signatures and error handling.
No successful guest flag, adapter reply or vehicle result is injected. The ten
hash-verified runnable catalog cards have identical executable bytes over card
offsets `0x798CAA..0x7B09B0`, covering the guarded communication/loader routines.
Their existing PC/signature guards were retained; no speculative profile added.

## Evidence and reproducibility

Contributor originals are linked from GitHub discussion 5, comment 18922485:
<https://github.com/djfremen/OpenSAAB-T2/discussions/5#discussioncomment-18922485>.
The welcome identifies Swedish 148.000. The logs do not contain a card hash:
they do not prove that the contributor used the exact catalog binary. Selecting
Swedish in our chooser selects a separate downloaded diagnostic image.

The catalog Swedish image SHA-256 is
`3af7891734a44a614ffdce2069756cf1ce9add3df13e65c903654fb38c684908`.
The former ARM64 .2 runtime reproduced `ADDRESS ERROR - PROCESSOR HALT` at PC
`0x171C6` after replaying the 19 original encoder events, with no physical adapter.
The earlier capture and the contributor's separate immediate English failure
must not be conflated. Exact current English card/run evidence is still needed.

`tests/firmware_startup_replay.py` checks all runnable card/support hashes, boots
the real images and asserts welcome readiness with no CANdi construction. Its
optional recorded-key replay intentionally omits CANdi firmware and checks that
the first dependency stops with an initialization error before a processor halt.
Raw diagnostic images and contributor logs remain outside this repository.

Example (all inputs and a fresh output directory must be provided):

```sh
python3 tests/firmware_startup_replay.py --binary /path/to/tech2-emu \
  --support /path/to/system --cards /path/to/catalog-images \
  --replay-keys /path/to/recorded-keys.txt --output /path/to/new-evidence
```

Unit coverage checks active/hidden text, translated prompts, English's alternate
welcome row, wrong manufacturer/header/version and missing ENTER. Existing guest
loader caller/restoration/failure tests and adapter suites remain regression gates.

These are research-harness firmware execution checks. They do not qualify native
boot fidelity, the installed GUI, real Nano firmware, Ventura/OCLP, diagnostics,
or transport cleanup on a physical device. Android preview.70 remains the pinned
reference; Android, Windows, Linux and iOS packaged adoption/qualification remain
pending. Historical packages and evidence are preserved.


## Apple silicon rebuild .2

Owner requested ARM64-only rebuild `0.2.25-candi-startup.2` for a later vehicle
test, with Intel promotion deferred until the remaining cause is established.
The .1 welcome correction and adapter capabilities are retained. Both additional
October14 Swedish key sequences reproduced the old halt; .1 reached guarded
initialization with an injected missing-firmware failure. Current English NAO
failure, active contributor card hash and screenshot remain unconfirmed.

This rebuild also adopts the new standing `security-access-password-1` policy.
The shared worker requires the exact case-sensitive password before preparation,
provider/fallback decisions and processed-response import. The macOS UI, native
HTTP path and automation call the same gate; invalid remembered credentials are
removed and re-prompted. Credentials stay out of transaction/receipt files and
fallback HTTP. Passive status/observation and existing diagnostics stay separate.
Exact installed worker/HTTP/isolated-Keychain evidence is required before claiming
the rebuilt policy verified. Other installed targets retain their historical
packages and need independent adoption/verification.
