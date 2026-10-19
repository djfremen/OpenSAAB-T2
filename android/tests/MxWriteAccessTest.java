// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class MxWriteAccessTest {
    private static void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public static void main(String[] args){
        check(MxWriteAccess.clearEntry("Clear Vehicle DTC's",4,false,4),"F1 Clear escaped preflight");
        check(MxWriteAccess.clearEntry("F1: Clear Vehicle DTC's",4,true,16),"Selected Enter escaped preflight");
        check(MxWriteAccess.clearEntry("Clear Diagnostic Information",4,false,4),"ECU clear escaped preflight");
        check(!MxWriteAccess.clearEntry("Clear Vehicle DTC's",4,false,16),"Unselected clear blocked another Enter");
        check(!MxWriteAccess.clearEntry("Clear Vehicle DTC's",4,true,12),"Navigation blocked");
        check(!MxWriteAccess.clearEntry("Vehicle Check and Read DTC's",24,true,24),"Read blocked");
        check(!MxWriteAccess.clearEntry("Transmission",4,true,4),"Unrelated F1 blocked");
        MxWriteAccess gate=new MxWriteAccess();check(gate.begin("read-A"),"Consent not retained");
        check(!gate.begin("read-B"),"Pending consent overwritten");
        check(!gate.consume("old-session",true,true)&&gate.pending(),"Stale completion consumed current consent");
        check(gate.consume("read-A",true,true),"Verified release did not hand off");
        check(!gate.consume("read-A",true,true),"Consent reused");
        gate.begin("read-A");check(!gate.consume("read-A",false,true),"Unconfirmed cleanup enabled writes");
        gate.begin("read-A");check(!gate.consume("read-A",true,false),"Background enabled writes");
        gate.begin("read-A");gate.cancel();check(!gate.consume("read-A",true,true),"Cancelled consent enabled writes");
        System.out.println("MX_WRITE_ACCESS PASS: F-key/Enter preflight, read/navigation preserved, stale/cancel/background/release and once-only consent");
    }
}
