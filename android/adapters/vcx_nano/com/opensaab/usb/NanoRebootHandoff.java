// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Single explicit reboot request, after the previous session releases its ownership. */
public final class NanoRebootHandoff {
    public enum State { IDLE, WAITING, READY, CONSUMED, CANCELLED, FAILED }
    public static final long DEADLINE_MS=15000;
    private State state=State.IDLE;
    private long started;
    private boolean requireCleanup;
    public boolean begin(long now,boolean requireCleanup){
        if(state==State.WAITING||state==State.READY)return false;
        started=now;this.requireCleanup=requireCleanup;state=State.WAITING;return true;
    }
    public State poll(long now,boolean active,boolean cleanupConfirmed,boolean foreground){
        if(state!=State.WAITING)return state;
        if(!foreground){state=State.CANCELLED;return state;}
        if(active){if(now-started>=DEADLINE_MS)state=State.FAILED;}
        else state=requireCleanup&&!cleanupConfirmed?State.FAILED:State.READY;
        return state;
    }
    public boolean consume(){if(state!=State.READY)return false;state=State.CONSUMED;return true;}
    public void cancel(){if(state==State.WAITING||state==State.READY)state=State.CANCELLED;}
    public State state(){return state;}
    public static boolean verified(String status,String origin,boolean adapterAck,boolean ownerAck,long vehicleTx,
            boolean usbClosed,boolean channelCleanup,int cleanupErrors,int diagnosticWrites){
        return "adapter_software_reboot_acknowledged".equals(status)&&"android-direct-usb".equals(origin)
            &&adapterAck&&ownerAck&&vehicleTx==0&&usbClosed&&channelCleanup&&cleanupErrors==0&&diagnosticWrites==0;
    }
}
