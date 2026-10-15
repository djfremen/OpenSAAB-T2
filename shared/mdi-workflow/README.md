# Shared native MDI workflow

Ported from verified Android ARM64 OpenSAAB0.1.0-preview.70 matching source.
ANDROID_REFERENCE.json pins the original module, protocol and bounded guest bridge.
The Rust protocol in shared/mdi-protocol intentionally adapts that pinned source;
shared carrier retains
RNDIS framing, userspace TCP, validated fresh multicast identity, built-in classic
MDI bootstrap/version gate, fresh VIN/cleanup and original guest lifecycle. Mac
changes are USB trait plumbing, native working-card/frame/input paths and typed
identity/cleanup evidence. The current emulator consumes the same guest backend,
including the preview.70 nonfatal bounded recorder.

macos_usb.c/macOS wrapper own IOKit device/interface ownership, configuration2,
control requests and whole asynchronous bulk transfers only. No root, Windows
vendor driver or host network route. Selected location and registry ID are both
revalidated before open. A pending IN completion remains allocated until abort
is reaped; cleanup failure cannot become confirmed release.

The app retains common Full native control, Read-only diagnostics and Collect
security seeds, firmware/LCD/key pumps, consent/SSA import and passive reports.
Physical ignition prompts remain owner actions. Existing Android is preserved;
its later shared-core cutover and every other target/architecture qualification
remain pending. This preview is not universal MDI2/RTR/firmware compatibility.

The .2 guest bridge adds transport-neutral policy-denial detail to the shared
classifier. Read-mode Check Ignition Key Status remains outside the read profile;
its original CIM AE request is locally blocked and directs owners to Full native
control. This does not broaden permissions or imply an ECU rejection.

The .3 native carrier, firmware supervisor and guest bridge remove the Android
reference's blanket 870/900-second session-age shutdown. Explicit operator Stop,
carrier-fault cancellation and guest process exit retain normal cleanup. Identity
retrieval remains bounded at eight seconds, management and individual TCP
operations keep their deadlines, and shutdown keeps its 45-second forced-exit
grace. Queue limits and the nonfatal 128 MiB guest capture limit remain in place;
the Read, Seeds and Full permissions are unchanged. The pinned Android source
still has its historical lifetime caps; Android adoption of this lifetime policy
and the shared implementation remains pending.

Only the native MDI guest opts into `RawPair::open_live` and the manual frontend's
operator-controlled lifetime. The original `RawPair::open`, standalone RawSession
and other backends retain their historical probe/session limits. Live RPCs keep
their two-second operation deadline and TX indications keep their two-second
ticket deadline; live mode removes only the accumulated session-age limit.
Cleanup restores its finite overall deadline. Live notifications retain the
128-record poll bound and pending-TX/transport queue bounds, but past notification
count cannot terminate a healthy session. Activity awaiting collection is capped
at 256 KiB; overflow drops recorder entries with an explicit counter, without
stopping transport. Taking activity releases those records and resets the bound.
