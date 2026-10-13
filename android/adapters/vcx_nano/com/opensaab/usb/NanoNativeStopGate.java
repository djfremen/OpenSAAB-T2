// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Bounded native Stop grace. Never acknowledges an untransmitted guest CAN frame. */
public final class NanoNativeStopGate {
    public static final long GRACE_MS=25000;
    private static final String[] CLEANUP={
        "BB800048010100010100CCBB","BB80004301C4BB","BB80004101C2BB",
        "BB800048000100010100CBBB","BB80004300C3BB","BB80004100C1BB"
    };
    private long started=-1;
    private boolean quit;
    private int rejected;
    private final boolean[] delivered=new boolean[CLEANUP.length];
    public synchronized void reset(){started=-1;quit=false;rejected=0;java.util.Arrays.fill(delivered,false);}
    public synchronized boolean begin(long now){if(now<0)throw new IllegalArgumentException("Invalid monotonic clock");if(started>=0)return false;started=now;return true;}
    public synchronized boolean pending(){return started>=0;}
    public synchronized boolean awaitingQuit(){return started>=0 && !quit;}
    public synchronized boolean expired(long now){return started>=0 && (now<started || now-started>=GRACE_MS);}
    /** Permit each exact existing shutdown control at most once, in either channel order. */
    public synchronized boolean cleanupAllowed(String wire,long now){
        if(!awaitingQuit() || expired(now))return false;
        for(int i=0;i<CLEANUP.length;i++)if(CLEANUP[i].equals(wire)){
            if(delivered[i])return false;delivered[i]=true;return true;
        }
        return false;
    }
    /** Queued original requests get ERROR stopped, while their caller can still close. */
    public synchronized boolean rejectQueued(long now){return awaitingQuit() && !expired(now) && rejected++<32;}
    public synchronized void quitReceived(){quit=true;}
}
