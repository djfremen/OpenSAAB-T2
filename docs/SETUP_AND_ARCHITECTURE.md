# Setup and architecture direction — 2026-09-15

Use a single OpenSAAB Setup entry point: check Android application ABI support, memory and storage, select the matching separately maintained ARM64 or ARMv7 APK, and guide software/language selection. Download verification, Android installation approval and a safe firmware handoff are required. The existing checker currently routes users to release pages; the unified download/install/firmware handoff is not implemented yet.

The emulator architecture indicator reads the installed `libtech2_emu.so` ELF header (class and ARM machine type). It displays `Emulator: 32-bit ARM` or `Emulator: 64-bit ARM`. A missing, malformed, non-ARM or inconsistent executable displays `architecture unavailable`; it never guesses based on the processor name or the Java application's process. A 64-bit-capable CPU can have 32-bit-only Android. This describes the host executable, not the emulated Tech2 guest CPU.

The shared branded header shows this indicator on main/Chipsoft/setup screens; the Nano screen has the same indicator. The architecture does not change automatically inside a running app. The installed APK supplies the executable.

Validation: Java ELF-header tests (ARMv7, ARM64, inconsistent headers, non-ARM and missing files), inspected both locally built native targets, and successful ARM32 APK build. These source changes are not yet in the published headunit.3 release.
