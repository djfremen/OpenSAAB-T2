# Explicit T8 counter reset candidate

Candidate 0.1.1-preview.1 adds **Reset misfire and knock counters** to Actions.
It starts original firmware menus. The owner selects the vehicle and original
Reset Misfire and Knock Counters operation and follows its instructions.

Nano uses a separate `ResetEngineCounters` profile. In addition to the existing
diagnostic reads, it admits only standard HS-CAN ID `0x7E0` with the captured
eight bytes `02 AE 14 00 00 00 00 00`. Read, seed collection and DTC clearing
do not inherit this permission. Rust and the Android USB owner both validate
the request; a distinct handshake binds the selected maintenance profile.
The existing exact three-request ignition-key-status read is retained for the
firmware's prerequisite key-position check. No host-generated reset, arbitrary DeviceControl, security key, programming
or download operation is introduced. Existing stop, cleanup and session limits
remain in force. Nano support for other engines or reset requests is unavailable.

Chipsoft full control and MDI retain their original firmware workflows. An
explicit Chipsoft restricted session cannot start the reset action.

Physical result requires a fresh original-firmware ECU response, firmware
completion and subsequent counter readings on the exact installed candidate.
USB acceptance alone is insufficient. Fire tablet Nano, MDI and Chipsoft live
checks are pending until individually recorded. Other platforms and ARM32
have no installed adoption or qualification claim from this candidate.

This is a local test candidate. No release or update catalog is published.
