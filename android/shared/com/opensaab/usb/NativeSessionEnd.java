// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Session termination and adapter cleanup never establish an ECU workflow result. */
public final class NativeSessionEnd {
    private NativeSessionEnd(){}
    public static String message(String adapter,Integer exit,boolean forced,boolean requested,String usb,String protocol){
        String end=forced?"session forcibly stopped":exit!=null&&exit!=0?"session failed (exit "+exit+")":requested?"session stopped":"session ended";
        return adapter+" "+end+" · Changes not verified; check saved session"
            +" · USB "+status(usb)+" · Protocol cleanup "+status(protocol);
    }
    private static String status(String value){return "succeeded".equals(value)?"confirmed":"failed".equals(value)?"failed":"unconfirmed";}
}
