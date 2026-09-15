// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Formatting is fail-closed even if the native process freezes or the clock moves. */
public final class IgnitionStatusText {
    private static String name(String state) {
        return "on".equals(state)?"ON":"lock".equals(state)?"LOCK":"Unknown";
    }
    public static String format(String state,String freshness,String last,long age,long sincePublish,boolean active,boolean connected) {
        if(!active || !connected)return "Ignition: Unknown · disconnected";
        if(age<0)return "Ignition: Unknown · awaiting CIM reply";
        if(sincePublish<0 || sincePublish>60000)return "Ignition: Unknown · telemetry unavailable";
        long total=age+sincePublish;
        if(total<age)return "Ignition: Unknown · telemetry unavailable";
        String elapsed=String.format(java.util.Locale.ROOT,"%.1fs",total/1000.0);
        if(!"fresh".equals(freshness) || total>2000)
            return "Ignition: Unknown · stale (last "+name(last)+", "+elapsed+" ago)";
        return "Ignition: "+name(state)+" · CIM/CAN · "+elapsed+" ago";
    }
}
