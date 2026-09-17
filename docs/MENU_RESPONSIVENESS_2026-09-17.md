# Offline menu responsiveness

The offline MainActivity used one 120 ms loop to deliver keys and decode live.ppm, including unchanged files. The native interactive frontend polled input and published frames on the same 100 ms cadence. Each arrow could also copy the entire guest recovery state.

The offline path now uses:

- A bounded input worker that wakes on enqueue, publishes one atomic mailbox at a time and waits for consumption independently of rendering. Arrow events older than 500 ms expire before publication; Enter and Exit preserve FIFO order. Cancellation joins before the session publishes stop.
- The same event-driven NativeLcdPump as the Chipsoft display: one decode worker, one pending UI frame, changed-file observation and a one-second missed-event recovery check. Unchanged files do not allocate new bitmaps.
- A 10 ms native input cadence, separate 33 ms frame cadence and one-second log-limit checks. Pending guest down/up events apply backpressure to the input mailbox. Stop bypasses that backpressure.
- Recovery checkpoints before action keys, rather than on up/down navigation. Guest key encoder and firmware handling are unchanged.
- Console text retained in a bounded buffer, rendered in batches only while visible.

The live Chipsoft USB/security transport and its existing immediate input writer are unchanged. These changes do not infer successful vehicle security access or modify SSA handling.

`OpenSaabPerf` logcat entries contain input enqueue/publication time and framebuffer file/publication/application time. Native KEYPAD console lines contain consumption wall time. These timestamps help separate Android delivery, native processing and presentation. They do not contain vehicle identifiers or security payloads. Rendering metrics alone cannot measure guest response.

Tests include the native mailbox busy/stop behavior, action checkpoint selection, Android input backpressure/expiry/order/cancellation, and FileObserver frame delivery, unchanged-frame suppression, malformed frames, session switching and closure. A separately installed temporary UI probe can measure finger-release-to-visible-highlight on the actual Pixel without instrumentation restarting OpenSAAB. Use only a confirmed offline menu, alternating arrows; never select a vehicle operation. Such measurements include screenshot sampling overhead.
