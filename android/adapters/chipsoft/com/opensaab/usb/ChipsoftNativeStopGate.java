// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Stop preserves the bridge for exact existing channel/device teardown, with bounded fallback. */
public final class ChipsoftNativeStopGate {
    public static final long GRACE_MS=25000;
    private static final String[] CLEANUP={"050004000000880008800000","050004000000050005000000","2000000000000000"};
    private long started=-1;private boolean quit;private int rejected;
    private final boolean[] delivered=new boolean[CLEANUP.length];
    public synchronized void reset(){started=-1;quit=false;rejected=0;java.util.Arrays.fill(delivered,false);}
    public synchronized boolean begin(long now){if(started>=0)return false;started=now;return true;}
    public synchronized boolean pending(){return started>=0;}
    public synchronized boolean awaitingQuit(){return pending()&&!quit;}
    public synchronized boolean expired(long now){return pending()&&(now<started||now-started>=GRACE_MS);}
    public synchronized boolean cleanupAllowed(String wire,long now){
        if(!awaitingQuit()||expired(now))return false;
        for(int i=0;i<CLEANUP.length;i++)if(CLEANUP[i].equals(wire)){if(delivered[i])return false;delivered[i]=true;return true;}
        return false;
    }
    public synchronized boolean rejectQueued(long now){return awaitingQuit()&&!expired(now)&&rejected++<32;}
    public synchronized void quitReceived(){quit=true;}
}
