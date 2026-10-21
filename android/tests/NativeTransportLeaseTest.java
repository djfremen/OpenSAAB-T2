// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class NativeTransportLeaseTest {
    private static final class Transport {int closes,opens=1;}
    private static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    private static NativeTransportLease<Transport> lease(Transport t){return new NativeTransportLease<>("attempt-1","adapter-1",false,t,v->v.closes++);}
    private static void rejected(Runnable r){try{r.run();throw new AssertionError("Invalid handoff accepted");}catch(IllegalStateException expected){}}
    public static void main(String[] args){
        Transport t=new Transport();NativeTransportLease<Transport> l=lease(t);
        check(l.claim("attempt-1","adapter-1",false)==t,"Handoff replaced the live socket");
        check(t.opens==1&&t.closes==0,"Handoff reopened or closed the socket");
        l.close();check(t.closes==0,"Discovery closed the socket after native adoption");
        rejected(()->l.claim("attempt-1","adapter-1",false));
        for(int scenario=0;scenario<4;scenario++){
            Transport resource=new Transport();NativeTransportLease<Transport> pending=lease(resource);
            if(scenario==0){pending.close();rejected(()->pending.claim("attempt-1","adapter-1",false));}
            if(scenario==1)rejected(()->pending.claim("stale-attempt","adapter-1",false));
            if(scenario==2)rejected(()->pending.claim("attempt-1","different-adapter",false));
            if(scenario==3)rejected(()->pending.claim("attempt-1","adapter-1",true));
            pending.close();check(resource.closes==1,"Unclaimed/invalid socket was not closed exactly once");
        }
        System.out.println("NATIVE_TRANSPORT_LEASE PASS: same open socket, one consumer, no reconnect, cancel/stale/device/connection-mode rejection and exact release");
    }
}
