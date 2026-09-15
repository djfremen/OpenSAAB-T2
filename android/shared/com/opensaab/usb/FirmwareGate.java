// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;

/** Exclusive image changes; emulator/USB sessions retain a lease through cleanup. */
public final class FirmwareGate {
    private static int readers;
    private static boolean editing;
    public static synchronized Lease use() throws IOException {
        if(editing)throw new IOException("Firmware installation in progress. Retry when it finishes.");
        readers++;return new Lease(false);
    }
    public static synchronized Lease change() throws IOException {
        if(editing || readers!=0)throw new IOException("Stop the firmware and release USB before changing images.");
        editing=true;return new Lease(true);
    }
    public static synchronized boolean busy(){return editing;}
    public static synchronized boolean sessionActive(){return editing || readers!=0;}
    public static final class Lease implements AutoCloseable {
        private final boolean writer;private boolean closed;
        private Lease(boolean writer){this.writer=writer;}
        public void close(){synchronized(FirmwareGate.class){if(closed)return;closed=true;if(writer)editing=false;else readers--;}}
    }
}
