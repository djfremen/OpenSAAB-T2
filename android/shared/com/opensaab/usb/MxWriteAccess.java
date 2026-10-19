// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Foreground consent starts a new bounded clearing session after verified release. */
public final class MxWriteAccess {
    private String pending;
    public static boolean clearEntry(String label,int encoder,boolean selected,int key){
        if(label==null)return false;
        String text=label.trim().replaceFirst("^F[0-9]+:\\s*", "").toLowerCase(java.util.Locale.ROOT);
        boolean clear=text.startsWith("clear ")&&(text.contains("dtc")||text.contains("diagnostic trouble")||text.contains("diagnostic information"));
        return clear&&(key==encoder||(key==0x10&&selected));
    }
    public boolean begin(String session){if(pending!=null||session==null||session.isEmpty())return false;pending=session;return true;}
    public boolean pending(){return pending!=null;}
    public void cancel(){pending=null;}
    public boolean consume(String endedSession,boolean cleanupConfirmed,boolean foreground){
        if(pending==null||!pending.equals(endedSession))return false;
        pending=null;return cleanupConfirmed&&foreground;
    }
}
