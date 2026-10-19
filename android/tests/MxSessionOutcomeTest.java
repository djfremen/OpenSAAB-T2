// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class MxSessionOutcomeTest {
    private static void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public static void main(String[] args)throws Exception{
        String recorded="\u001b[33m[ 37.190s | CANDI] [WARN ] Native adapter stopped: STN original-firmware session policy rejected request\u001b[0m";
        check(MxSessionOutcome.rejectedLine(recorded),"Captured owner rejection lost in ANSI log");
        for(String other:new String[]{"Native adapter stopped: STN transmit queue full/closed","Native adapter stopped: Bluetooth disconnected","Emulation stopped unexpectedly", ""})
            check(!MxSessionOutcome.rejectedLine(other),"Real/unknown failure suppressed: "+other);
        check(!MxSessionOutcome.rejectedLine(null),"Missing evidence classified as rejection");
        check(MxSessionOutcome.unsupported(true,true,false,false),"Verified MX+ rejection became crash");
        check(!MxSessionOutcome.unsupported(false,true,false,false),"Other adapter failure suppressed");
        check(!MxSessionOutcome.unsupported(true,false,false,false),"Unexplained failure suppressed");
        check(!MxSessionOutcome.unsupported(true,true,true,false),"Forced termination suppressed");
        check(!MxSessionOutcome.unsupported(true,true,false,true),"Startup failure suppressed");
        if(args.length>0){
            String log=new String(java.nio.file.Files.readAllBytes(java.nio.file.Paths.get(args[0])),java.nio.charset.StandardCharsets.UTF_8);
            check(java.util.Arrays.stream(log.split("\n")).anyMatch(MxSessionOutcome::rejectedLine),"Real owner console not recognized");
        }
        System.out.println("MX_SESSION_OUTCOME PASS: real/ANSI gate rejection; missing, network, unknown, startup, forced and other-adapter failures stay distinct");
    }
}
