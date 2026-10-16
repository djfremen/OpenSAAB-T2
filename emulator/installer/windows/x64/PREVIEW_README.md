# OpenSAAB 0.2.18-preview.1 x86_64

Extract this portable preview into a new folder and launch
`OpenSAAB-0.2.18-preview.1-windows-x86_64.exe`. Its window and program name read
their full version, architecture and source revision from packaged metadata.
Keep historical packages; stop an existing session before launching this one.
The normal WindowsData folder, firmware, settings and retained reports are reused.

This preview recognizes the original Saab 148.000 welcome screens, including
translated cards, before arming deferred CANdi startup. It also corrects the
hard-coded Windows report version and records the launching app identity in
each new session's local transport evidence. A newly prepared report can still
include older retained sessions; its top-level version identifies the preparer.

Security processing, fallback and processed-response import require the
case-sensitive `OpenSAAB` password through the shared worker boundary. Stored
credentials are validated before use. A password or POST-AUTH label does not
establish that a vehicle module accepted security access.

The original direct Chipsoft route and explicit OEM comparison route are
preserved. No SPS, used-CIM/key prerequisite, receive-FIFO or cold adapter-startup
fix is claimed. The new package needs separate physical vehicle qualification.
Windows 11 ARM/Fusion runs this x64 application under emulation; that does not
qualify native ARM64 drivers or every Windows x64 computer.

No diagnostic card is included. Choose/download the desired card in the app.
Review Report issue JSON before sending. Preserve failed-run files privately
when full diagnostic context is needed; public reports exclude raw traffic,
security data and VINs. Matching source, manifest, hashes and scoped test results
are provided with the release.
