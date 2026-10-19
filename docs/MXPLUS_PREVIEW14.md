# MX+ preview.14 receive scheduling

The exact preview.13 reset capture contains Loss of Communications before the
reset, followed by a separate Negative Response screen (service 0 / code 0).
The ECU's EE14 reply reached original CANdi after its earlier D5/00/00 result.
The native transcript shows long SW listening intervals between original guest
requests. The observations support a bus-scheduling/latency failure; they do not
establish a universal 100 ms deadline or prove that every warning has one cause.
Virtual CANdi FIFO-overrun records remain a separate open observation.

Preview.14 remembers the bus of the last guest frame other than the exact
functional tester-present, standard ID 101 and FE013E0000000000. After that
tester completes on SW-CAN, an idle worker returns to HS monitoring only when
the last other guest frame was on HS. A queued guest request takes precedence.
Startup wake and addressed SW/CIM exchanges therefore retain SW reception.
This is receive scheduling: no guest bytes, flow control, replies, confirmations
or operation results are synthesized. The adapter still receives one bus at a
time; short physical receive gaps and reliable operation require live retesting.

Bus-switch setup uses one batch containing the previous ordered configuration
and protocol/bit-rate readbacks. Every verbose response, boundary and final
prompt must match before any subsequent CAN transmission. An incomplete,
malformed or rejected batch makes in-band transport uncertain and stops further
writes; only local ownership close remains permitted. No empty replay, automatic
key/request retry or broader diagnostic permission is added. STBC/STBCOF remain
separate setup commands. See the vendor's [FRPM F, section 16](https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf#page=73).

An owned byte peer emits a synthetic 5E8 stream at 300 ms intervals. Disabling
idle restoration reproduces loss of HS reception without a later HS transmit;
the production path resumes reception. Additional cases preserve addressed SW
key-status reception and reject wrong bit rate, missing/extra boundaries, early
or missing prompts and failed HS restoration without replay. These are software
regressions, not Fire/Bluetooth timings or a qualified vehicle workflow.

The full preview identity is 0.1.1-preview.14 ARM64. The six pinned legacy
USB/connection workers remain unchanged. The recovered and dedicated MX+
workers have this candidate's identity; previous artifacts and receipts remain
immutable. No installation, physical pass, publication or common cross-platform
qualification is implied by this source adoption.
