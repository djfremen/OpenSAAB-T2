// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;

/** One bounded RFCOMM attempt; a completed attempt cannot be closed by a stale timer. */
final class BluetoothConnectDeadline implements AutoCloseable {
    // The physical Fire/MX+ connected in 10.314 s. Eight seconds truncates valid
    // Android SDP/RFCOMM establishment (Android documents about 12 s).
    static final long TIMEOUT_MS = 20000;
    private final ScheduledFuture<?> timer;
    private boolean pending = true;

    BluetoothConnectDeadline(ScheduledExecutorService scheduler, Runnable expired) {
        timer = scheduler.schedule(() -> {
            synchronized (this) {
                if (!pending) return;
                pending = false;
                expired.run();
            }
        }, TIMEOUT_MS, TimeUnit.MILLISECONDS);
    }

    @Override public synchronized void close() {
        pending = false;
        timer.cancel(false);
    }
}
