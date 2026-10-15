# Intel Nano VIN pending reply correction

Candidate: **0.2.26-intel-preview.4**, based on the exact `.3` source
`d6f1ab8c50e9417e97317877653da7ec6914b8ab`.

A live native Intel Nano startup received a standard eight-byte service-1A
negative reply with NRC `0x78` before the VIN first frame. The compatibility
`VinReply` parser incorrectly treated response pending as a rejection, stopping
before the original Swedish Saab 148.000 firmware could launch. This is an
observed startup blocker; it does not establish the cause of the contributor's
reported Swedish firmware crash.

The shared parser now accepts matching response-pending replies only before
segmented VIN reception begins. Pending replies produce neither a VIN nor a
flow-control request. The Nano caller retains its existing four-second absolute
deadline, ECU/channel filtering, one request and first-frame flow control, fresh
identity validation and acknowledged channel/USB cleanup. Malformed replies,
other services/NRCs and pending replies during segmented reception fail. Errors
remain terminal. No security-access gate or guest-firmware traffic is changed.

The Android reference's `VlinkerProtocol.parseVin`, the shared vLinker parser
and the generic shared host transaction already distinguish response pending.
This correction applies that behavior to the compatibility helper rather than
adding a macOS-only interpretation.

Regression coverage includes repeated pending replies followed by a synthetic
17-character VIN, no premature flow control, wrong service/NRC/size/prefix,
out-of-order segments and terminal failures. Shared host deadline tests retain
the absolute time limit. Exact installed Intel and live vehicle evidence is
recorded separately; source tests do not establish physical qualification.

Reverse-engineering integrations, recovered-core work and other adapter
qualifications retain the scope recorded for `.3`. This focused candidate does
not adopt unrelated canonical working-tree changes. Historical packages remain
unchanged. No public release or recommendation is implied by this local test.
