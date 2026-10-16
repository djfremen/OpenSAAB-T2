# Interface convergence candidates — September 20

These changes implement part of the shared Android-reference contract. They are
**tested interface candidates, not a uniform release**. Linux 0.2.0 is installed
in the Omarchy x86_64 VM and Mac 0.2.2 is installed on the M4. Previous installations
were retained for rollback. Exact package hashes, source revisions and local
evidence hashes are recorded in `INTERFACE_CANDIDATE_RECEIPT_2026-09-20.json`.

## Linux 0.2.0

Replaces the old engine window as the entry point with an Android-reference
workspace: ready/vehicle state, Connect and start and offline entry, 4:3 LCD,
S1–S4, EXIT/Keypad/Actions, full keypad, app menu, console, help, expand/restore,
and compact/wide layouts. The implementation reads colors, labels and key encoder
identities from the shared JSON contract. Keyboard/mouse and on-screen firmware
controls use the real engine mailbox; EXIT does not terminate the session.

Firmware selection consumes the shared diagnostic catalog. Existing local runtime
and cards migrate without modifying the originals. Catalog images are verified
before atomic selection; downloads have cancellation, size bounds and timeouts.
Each session gets its own card copy. The app states that its controls use English.

Tkinter and Pillow 9.1+/ImageTk are system dependencies, checked before replacing an
existing install. No Rust compiler is needed by the end user. The builder can reuse
the earlier manifest-verified x86_64 engine, with separate engine source, artifact
and binary hashes. Its earlier Omarchy qualification does not qualify the new GUI.

Nine headless tests passed: verified activation, corrupt archive/blocked image
rejection, cancellation preservation, extraction-size/image-hash checks, legacy
card preservation, truthful unknown-card migration, invalid catalog paths,
complete-frame decoding, and real child-process key/stop behavior. All nine also
passed inside Omarchy, together with isolated installation, manifest verification,
repair, uninstall, retained data, reinstall and unrelated-file protection. The
installed package preserves the existing runtime. Omarchy required its supported
update workflow, then the official tk and python-pillow packages. Its desktop shell
reported a restart-readiness warning; the terminal and app remained usable.

Visual testing found and corrected missing keypad keys and initial dialog
positioning. The revised shell uses explicit dialog windows, keeps them within
the display, and describes offline mode in the vehicle header. The installed
revised package visibly reaches the model-year menu, responds to firmware EXIT,
and rearranges between wide and compact views without cropping the framebuffer.
The imported legacy card is truthfully identified as local when its hash does
not match a catalog image. The final keypad/dialog retest is still pending: UTM pointer delivery became
unreliable (requested clicks did not consistently reach the visible controls).
Keyboard firmware navigation still worked; this does not establish whether any
remaining dialog problem belongs to the app or the VM input path. Capture was
released and the VM was returned to its desktop. Complete visual conformance is
still pending.

No live Linux adapter transport is implemented. Reference menu entries explain
unsupported operations without issuing adapter commands. This is an explicit
functional gap, not an implementation pass for those actions.

## macOS 0.2.2

Adds compact header scrolling and wide/expanded arrangements using the reference
720-point threshold and orientation rule. The header rail is 248 points; controls
have 48-point minimum height. App/Actions menu labels and ordering now follow the
reference; Stop is separate from firmware EXIT. The keypad sheet was enlarged to
accommodate the control heights. Existing native identity probing is preserved.

The packaged and installed shell passes eight core checks, and all three installed
binary hashes match its receipt. Visual testing caught bottom-aligned header
content and native capsule buttons; the final package fixes both, uses the reference
button colors and full 48-point control faces, and gives the keypad its full width.

The installed app visibly ran German firmware, entered Diagnostics, moved model-year
selection, returned with EXIT, expanded/restored the display, scrolled the compact
header, stopped through the app menu, restored English, and restarted to the real
Main Menu. Screenshots cover wide, compact, scrolled, expanded, keypad and language
restoration states. Cached image verification/activation was observed; this is not
a fresh-download/cancellation UI pass. Live Mac VIN/DTC and advanced actions remain
explicitly unsupported. Rollback: `~/.local/share/opensaab/macos-shell-20260920/rollback-0.2.1/OpenSAAB.app`.

## Windows 0.2.4

Aligns the responsive rule with Android (720 logical units and width greater than
height), normalizes reference action labels and app menu entries, and hides Stop
when no session is running. The existing Chipsoft HS-CAN read and native driver
buckets are preserved. Missing security access, clearing, updates and direct app
navigation remain explicitly described. The build embeds the interface contract
and emits its hash with the firmware catalog hash.

All five shell source files compile against .NET Framework 4.8 reference assemblies
on the Mac with zero warnings/errors. This is a compile check, not a Windows launch,
MSI build, driver test or final installed qualification. Native MSI build and the
Windows 11 ARM walkthrough remain pending.

## Next verification

Complete the remaining Linux language download/cancellation cases. Build/install
the Windows MSI in its supported build environment and run the same sequence.
The previously known remote Windows build connection did not accept authentication
during this pass; no Windows installation is claimed. Then close remaining action/transport gaps
with the physical Chipsoft bench. Do not mark any full conformance row passed on
the strength of these code tests.
