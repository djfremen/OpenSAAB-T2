// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Fake clocks and protocol responses; no worker, socket, USB or Android effect. */
public final class NanoNativeStopGateTest {
    static int checks;
    static void check(boolean ok){checks++;if(!ok)throw new AssertionError("check "+checks);}
    public static void main(String[] args){
        NanoNativeStopGate gate=new NanoNativeStopGate();
        check(!gate.pending());check(!gate.awaitingQuit());check(!gate.cleanupAllowed("BB80004100C1BB",0));
        check(gate.begin(1000));check(gate.pending());check(gate.awaitingQuit());check(!gate.begin(2000));
        check(!gate.expired(25999));check(gate.expired(26000));check(gate.expired(999));
        // A queued diagnostic command is rejected without closing or granting USB/TXOK.
        for(String queued:new String[]{"BB800040010000810143BB","BB80008C000CBB","BB8000A00000BB","BB8001000100000000000C0000024103AE03020000000087BB"}){
            check(!gate.cleanupAllowed(queued,2000));check(gate.rejectQueued(2000));check(gate.awaitingQuit());
        }
        for(String cleanup:new String[]{"BB800048010100010100CCBB","BB80004301C4BB","BB80004101C2BB","BB800048000100010100CBBB","BB80004300C3BB","BB80004100C1BB"}){
            check(gate.cleanupAllowed(cleanup,2000));check(!gate.cleanupAllowed(cleanup,2001));
        }
        gate.quitReceived();check(gate.pending());check(!gate.awaitingQuit());check(!gate.rejectQueued(2002));check(!gate.cleanupAllowed("BB80004100C1BB",2002));
        gate.reset();check(!gate.pending());check(gate.begin(0));
        // Partial startup can close only an attempted HS channel, with no six-control claim.
        check(gate.cleanupAllowed("BB800048000100010100CBBB",1));check(gate.cleanupAllowed("BB80004300C3BB",1));check(gate.cleanupAllowed("BB80004100C1BB",1));gate.quitReceived();check(!gate.awaitingQuit());
        gate.reset();check(gate.begin(5));for(int i=0;i<32;i++)check(gate.rejectQueued(6));check(!gate.rejectQueued(6));
        gate.reset();check(gate.begin(5));check(!gate.cleanupAllowed("BB80004100C1BB",25005));check(!gate.rejectQueued(25005));
        gate.reset();check(gate.begin(Long.MAX_VALUE-10));check(!gate.expired(Long.MAX_VALUE));check(gate.expired(0));
        System.out.println("NANO_NATIVE_STOP_GATE PASS checks="+checks+"; hardware not opened");
    }
}
