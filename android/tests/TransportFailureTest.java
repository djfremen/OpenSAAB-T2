package com.opensaab.usb;
public final class TransportFailureTest {
    public static void main(String[] args){
        String tx="CAN TX intent controller=2 bus=SingleWire wake_control_selected=true id=0x00000100 dlc=0 data=[]\n";
        String stop="Native adapter stopped: Chipsoft opcode=000F status=001D\n";
        if(TransportFailure.explain(tx+stop)==null)throw new AssertionError("Known wake failure missing");
        for(String trace:new String[]{stop,tx,tx.replace("SingleWire","HighSpeed")+stop,tx+stop.replace("001D","0001"),tx+"CAN TX intent bus=HighSpeed\n"+stop})
            if(TransportFailure.explain(trace)!=null)throw new AssertionError("Unrelated failure misclassified");
        System.out.println("Transport failure: exact rejected single-wire wake and unrelated failures PASS");
    }
}
