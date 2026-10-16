# Adapter code buckets

- `chipsoft/`: Chipsoft Pro protocol, channels, backend, identity probe and macOS serial transport.
- `vcx_nano/`: VCX Nano protocol, channels and native backend.
- `j2534/`: Windows bridge shared by the selected vendor driver.
- `common/`: bounded byte transport and shared firmware request policy, without vendor command sequences.

See [adapter architecture and initialization contract](../../docs/adapters/README.md). Public legacy module names are compatibility re-exports in `src/lib.rs`.
