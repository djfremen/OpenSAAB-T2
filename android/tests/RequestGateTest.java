// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class RequestGateTest {
    private static void check(boolean condition,String message) {
        if(!condition)throw new AssertionError(message);
    }
    public static void main(String[] args) {
        RequestGate gate=new RequestGate();
        long first=gate.begin();check(gate.pending(first),"new request missing");
        gate.cancel();check(!gate.consume(first),"grant after Stop started work");
        long second=gate.begin();check(!gate.consume(first),"stale grant consumed newer request");
        check(gate.consume(second),"current grant refused");
        check(!gate.consume(second),"duplicate grant started second worker");
        long third=gate.begin();gate.cancel();
        check(!gate.consume(third),"background cancellation did not invalidate grant");
        long fourth=gate.begin(),fifth=gate.begin();
        check(!gate.consume(fourth) && gate.consume(fifth),"superseded request admitted");
        RequestGate recreated=new RequestGate();long replacement=recreated.begin();
        check(!recreated.consume(fifth) && recreated.consume(replacement),"old Activity grant started recreated Activity");
        System.out.println("REQUEST_GATE_TESTS PASS; Android/USB/hardware not opened");
    }
}
