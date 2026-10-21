// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.function.Consumer;

/** One retained transport, one exact discovery attempt, no reconnect or replay. */
final class NativeTransportLease<T> implements AutoCloseable {
    private final String attempt, address;
    private final boolean compatibility;
    private final Consumer<T> release;
    private T transport;
    NativeTransportLease(String attempt,String address,boolean compatibility,T transport,Consumer<T> release){
        if(attempt==null||attempt.isEmpty()||address==null||address.isEmpty()||transport==null||release==null)throw new IllegalArgumentException("Incomplete native transport handoff");
        this.attempt=attempt;this.address=address;this.compatibility=compatibility;this.transport=transport;this.release=release;
    }
    synchronized boolean available(){return transport!=null;}
    synchronized T claim(String attempt,String address,boolean compatibility){
        if(transport==null)throw new IllegalStateException("Native transport handoff already consumed or cancelled");
        if(!this.attempt.equals(attempt)||!this.address.equals(address)||this.compatibility!=compatibility){close();throw new IllegalStateException("Native transport handoff selection changed");}
        T result=transport;transport=null;return result;
    }
    public synchronized void close(){T value=transport;transport=null;if(value!=null)release.accept(value);}
}
