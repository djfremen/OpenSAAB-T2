// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;

/** Keeps an unconfirmed physical SPP owner across higher-level workflow exits. */
final class SppTransportAdmission {
    interface Owner { boolean closeConfirmed(); }
    private Owner previous;
    private boolean constructing;
    private static final String BLOCKED="Previous Bluetooth connection has not finished closing. If cleanup failed, force stop OpenSAAB in Android settings, then reopen it before reconnecting.";

    synchronized void reserve(Owner next)throws IOException{
        if(next==null)throw new IllegalArgumentException("Missing SPP owner");
        // Query live completion rather than caching an early false receipt. A
        // writer still unwinding may become fully released without a restart.
        if(previous!=null&&(constructing||!previous.closeConfirmed()))throw new IOException(BLOCKED);
        previous=next;constructing=true;
    }
    synchronized void constructed(Owner owner){
        if(previous!=owner||!constructing)throw new IllegalStateException("SPP construction reservation changed");
        constructing=false;
    }
    synchronized void constructionFailed(Owner owner){
        // Only BluetoothSocket creation failure before a socket was returned
        // may abandon a reservation. There is no reset for a live/failed close.
        if(previous!=owner||!constructing)throw new IllegalStateException("Cannot abandon a constructed SPP owner");
        previous=null;constructing=false;
    }
    synchronized void requireOwner(Owner owner)throws IOException{
        if(previous!=owner||constructing)throw new IOException(BLOCKED);
    }
}
