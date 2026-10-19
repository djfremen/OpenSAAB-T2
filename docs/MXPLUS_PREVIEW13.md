# MX+ preview.13 immediate receive

Exact Fire preview.12 passed the startup key-position check. Its CIM VIN exchange received the first frame and CF22, but CF21 was absent from the raw STN log. The original firmware then reported a wrong vehicle/year. No counter reset was sent. This points to the receive gap between a transmit-only STPX command and a later STM command.

The owner's STN2255 v5.12.4 was separately checked with a uniquely versioned private helper: STBC1 succeeded and batched STI/STPR returned both read-only results. STBC0 and Bluetooth close completed. The helper sent no CAN request.

Preview.13 requires that volatile batching capability. Non-wake original guest frames use STPX R:0 followed by STM in a single batch. Exactly one pipe boundary acknowledges the transmit command before continuous receive; it is not a terminal prompt, electrical ACK or ECU success. Driver provenance is explicit. Original CANdi still owns every transmitted byte, flow-control frame and diagnostic interpretation. No host ISO-TP assembly or synthesized reply is added.

No capability, early prompt, duplicate boundary, CAN ERROR, STOPPED or missing boundary fails the session without replay or a queued next write. Zero-DLC wake remains the previous separately checked path. Permissions and six pinned workers are unchanged. Fast consecutive-frame byte-peer tests use synthetic data. Exact physical results and cleanup require separate installed evidence.

Vendor reference: [OBDLink Family Reference and Programming Manual F, section 16.1](https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf#page=73). Indefinite monitoring is allowed at the end of a batch. No public release or uniform cross-platform qualification is claimed.
