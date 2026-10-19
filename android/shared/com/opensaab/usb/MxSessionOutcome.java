// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Classifies the existing MX+ gate outcome; never grants or transmits a request. */
public final class MxSessionOutcome {
    private MxSessionOutcome(){}
    public static boolean rejectedLine(String line){
        return line!=null&&line.contains("Native adapter stopped: STN original-firmware session policy rejected request");
    }
    public static boolean unsupported(boolean mxSession,boolean rejected,boolean forced,boolean launchFailed){
        return mxSession&&rejected&&!forced&&!launchFailed;
    }
    public static String readSummary(){return "MX+ read-only session · Read codes and key status · Code clearing unavailable";}
    public static String unavailableMessage(){return "Operation unavailable in this MX+ session. The blocked request was not sent to the vehicle. Code clearing is not supported in this build.";}
}
