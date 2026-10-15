# OpenSAAB T2 — Open Connection

Use `opensaab-t2-logo-official.png` as the canonical logo. The Android launcher and header variants are in `android/tech2-app/res/` and `android/shared/com/opensaab/usb/BrandHeader.java`.

Preserve proportions; do not stretch the logo into a phone screen. A phone mockup should show a real app screenshot. Use only this approved logo in official release material. The source license does not grant trademark rights or allow a fork to imply official endorsement.

Desktop header assets use this same PNG. `opensaab-t2-logo-official.ico` is a
multi-resolution Windows icon derived from it (16–256 px); the Windows executable,
MSI app listing and shortcuts use that icon. macOS generates ICNS from the PNG;
Linux packages the PNG for both its desktop entry and visible header.

Voluntary project support wording and the confirmed Ko-fi destination are pinned
in `emulator/ui/interface-contract.json` under `branding.support`, copied from
Android's `project_support.xml`. Native renderers open the destination only after
the user selects Continue. No vehicle data is appended. Android ARM32/ARM64 share
`BrandHeader` and `ProjectSupport`; do not invent a platform-specific donation URL.
