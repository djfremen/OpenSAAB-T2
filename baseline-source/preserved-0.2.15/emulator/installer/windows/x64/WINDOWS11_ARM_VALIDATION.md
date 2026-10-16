# Windows 11 ARM smoke test — 2026-09-19

Host: M4 Mac mini, VMware Fusion, existing `Windows 11 64-bit Arm` VM (2 virtual CPUs, 4096 MB configured RAM). The VM is an existing development environment, not a clean Windows installation.

Prepared the same private `OpenSAAB-0.1.1-x64.msi` tested on the EliteBook, SHA-256 `DAD3800A426088B4712C1383B36E3007122BC95D0B141D0A7E0AF1A213BD960B`.

The user completed the installer media/launch interaction after a remote file-picker input problem. On resuming inspection, the OpenSAAB desktop shortcut and running TECH2-SCREEN welcome screen were visible. The original Saab firmware displayed software version 9.250, North American Operations. The installer wizard completion and installed-file hashes were not independently inspected in this session.

Verified interactively through VMware:

- Enter: welcome screen to Main Menu.
- Enter on Diagnostics: model-year selection.
- Down: selection changed from 2012 to 2011.
- Escape: returned to Main Menu.
- Event console recorded corresponding key and screen events; status showed no live adapter selected.

Result: successful basic emulator startup and menu navigation in this Windows 11 ARM VM. Left the emulator running at Main Menu. This does not establish adapter driver compatibility, live ECU communication, native ARM64 compilation, repair/uninstall behavior on ARM, or compatibility with all Windows 11 systems. No Chipsoft or VCX Nano was connected for this test.
