// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class IgnitionStatusTextTest {
    static void check(boolean ok){if(!ok)throw new AssertionError();}
    public static void main(String[] args){
        check(IgnitionStatusText.format("on","fresh","on",100,100,true,true).startsWith("Ignition: ON"));
        check(IgnitionStatusText.format("on","fresh","on",100,2000,true,true).contains("stale"));
        check(IgnitionStatusText.format("on","fresh","on",100,-1,true,true).startsWith("Ignition: Unknown"));
        check(IgnitionStatusText.format("on","fresh","on",100,0,false,true).contains("disconnected"));
        check(IgnitionStatusText.format("on","fresh","on",100,0,true,false).contains("disconnected"));
        check(IgnitionStatusText.format("unknown","unavailable","unknown",-1,0,true,true).contains("awaiting"));
        check(IgnitionStatusText.format("lock","fresh","lock",0,0,true,true).startsWith("Ignition: LOCK"));
        check(IgnitionStatusText.format("crank","fresh","crank",0,0,true,true).startsWith("Ignition: Unknown"));
        System.out.println("IGNITION_STATUS_TEXT PASS; stale/disconnected readings never presented as current ON");
    }
}
