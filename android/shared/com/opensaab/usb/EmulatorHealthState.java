// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Monotonic timing policy. A static display alone is never a failure. */
public final class EmulatorHealthState {
    public static final long STARTUP_MS=180000, HEARTBEAT_MS=30000, INPUT_MS=30000, UI_MS=20000;
    long started, heartbeat, input=-1, ui; boolean seenHeartbeat, alerted;
    public void reset(long now){started=heartbeat=ui=now;input=-1;seenHeartbeat=alerted=false;}
    public void heartbeat(long now){heartbeat=now;seenHeartbeat=true;}
    public void input(long now){if(input<0)input=now;}
    public void frame(){input=-1;}
    public void ui(long now){ui=now;}
    public String reason(long now){
        if(alerted)return null;
        if(now-ui>=UI_MS)return "ui_unresponsive";
        if(now-heartbeat>=(seenHeartbeat?HEARTBEAT_MS:STARTUP_MS))return "emulator_heartbeat_missing";
        if(input>=0&&now-input>=INPUT_MS)return "input_without_display_response";
        return null;
    }
}
