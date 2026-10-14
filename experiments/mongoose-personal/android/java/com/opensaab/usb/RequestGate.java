// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** UI-thread request identity. A cancelled or superseded permission callback
 * cannot start USB work, and a permission grant is consumed at most once. */
final class RequestGate {
    private static final java.util.concurrent.atomic.AtomicLong NEXT=
        new java.util.concurrent.atomic.AtomicLong(new java.security.SecureRandom().nextLong());
    private long epoch;
    private boolean requested;
    long begin() { requested=true;epoch=NEXT.incrementAndGet();return epoch; }
    void cancel() { requested=false;epoch=NEXT.incrementAndGet(); }
    boolean pending(long id) { return requested && id==epoch; }
    boolean consume(long id) {
        if(!pending(id))return false;
        requested=false;return true;
    }
}
