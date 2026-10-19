# MX+ preview.12 controller initialization

Exact Fire preview.11 reached the real menus and accepted remote navigation. On entering Engine Control, the guest sent its original zero-DLC SW wake and then wrote CANdi middle-controller register 0x00201001. That controller was not initialized in the MX+ path, so the model stopped. No normal diagnostic payload or counter reset was transmitted. Native Bluetooth cleanup was confirmed; private report/log archive is preserved.

Preview.12 initializes all three virtual controllers, matching the existing USB paths. The common native-link poller already leaves controller 1 physically disconnected, with TX pending and no ACK; it never sends that traffic to MX+, remaps the bus or invents a reply. HS and SW retain the same shared backend. Exact T8 reset policy and six pinned original workers are unchanged.

Software and exact installed/physical results are recorded separately. Previous packages are immutable. Shared primary-workflow integration and other platform/architecture qualification remain pending. No release is published by this local test.
