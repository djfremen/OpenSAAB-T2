# MX+ preview.15 discovery reception and request latency

Preview.14's faster bus setup shortened the interval from the functional HS
identification request to selection of SW from 135–160 ms to 38–51 ms in the
retained captures. Preview.13 caught EHU's identification response at 59 ms in
one of three attempts; none of eight preview.14 attempts caught it. The missing
EHU DTC query follows that discovery gap. A direct EHU query still replied.

After a policy-approved functional HS request other than tester-present, this
candidate preserves HS receive for 100 ms after its transmit completion boundary
before processing queued cross-bus work. The pending request retains its FIFO
position; same-bus work, physical ECU requests and flow control receive no such
hold. Cancellation can close the session during the hold. Reception continues
through the existing bounded socket reads; there is no extra vehicle request,
synthetic reply, frame change or replay. This is a scoped collection interval,
not simultaneous reception or a universal diagnostic deadline.

The Android MX+ byte bridge's idle receive wait is 10 ms instead of 50 ms. This
reduces the worker's queue pickup delay without adding another transport owner
or changing authentication, queues, cancellation or error handling. Ordinary
STPX requests already supply the CAN ID in H, making ATSH redundant. On HS,
STPX opens the selected protocol itself; separately checked STPO remains before
SW mode changes, and the previously verified wake sequence is retained. See
the vendor's [FRPM F, sections 8.6 and 8.14](https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf).

The original-firmware transmit/STM batch and configuration-batch validation
remain unchanged. No mixed setup/vehicle batch is needed for these reductions.
The default read profile, exact counter-reset exception, bounded key-status
exception and all packet-ID rules are unchanged. Entertainment's captured CIM
packet read remains unavailable in this candidate; expanding its policy needs
separate source evidence and qualification.

The owned byte peer injects an EHU reply at 60 ms while SW work is queued. The
regression requires that reply before the SW completion with unchanged request
bytes. It also checks cancellation during the hold and excludes tester-present,
physical requests and flow control. The existing periodic HS stream and failure
tests continue to apply. These are software tests, not a Fire/vehicle pass.

The exact installed identity is OpenSAAB 0.1.1-preview.15 ARM64. Six retained
USB/connection native workers remain pinned to preview.1. Changed Java byte
effects and dedicated/recovered MX+ workers belong to this candidate. Physical
qualification requires exact installed bytes, repeated discovery/DTC inventory,
the reset hold, a single authorized reset with same-clock timings, and key
status. Prior zero counters cannot prove nonzero-to-zero operation. FIFO
overruns and generic reporting context remain open. iOS implementation is on
hold until Android validation; no other target inherits this candidate's pass.
