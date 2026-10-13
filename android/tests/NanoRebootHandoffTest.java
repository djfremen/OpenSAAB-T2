// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class NanoRebootHandoffTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("check "+checks);}
    static boolean verified(String status,String origin,boolean[] flags,long tx,int errors,int writes){return NanoRebootHandoff.verified(status,origin,flags[0],flags[1],tx,flags[2],flags[3],errors,writes);}
    public static void main(String[] args){
        NanoRebootHandoff h=new NanoRebootHandoff();check(!h.consume());check(h.begin(100,true));check(!h.begin(101,true));
        check(h.poll(101,true,false,true)==NanoRebootHandoff.State.WAITING);check(!h.consume());
        check(h.poll(102,false,true,true)==NanoRebootHandoff.State.READY);check(h.consume());check(!h.consume());
        h=new NanoRebootHandoff();check(h.begin(0,true));check(h.poll(1,false,false,true)==NanoRebootHandoff.State.FAILED);check(!h.consume());
        h=new NanoRebootHandoff();h.begin(0,true);check(h.poll(NanoRebootHandoff.DEADLINE_MS-1,true,true,true)==NanoRebootHandoff.State.WAITING);check(h.poll(NanoRebootHandoff.DEADLINE_MS,true,true,true)==NanoRebootHandoff.State.FAILED);check(!h.consume());
        h=new NanoRebootHandoff();h.begin(0,false);check(h.poll(1,false,false,true)==NanoRebootHandoff.State.READY);h.cancel();check(!h.consume());
        h=new NanoRebootHandoff();h.begin(0,true);check(h.poll(1,false,true,false)==NanoRebootHandoff.State.CANCELLED);check(h.poll(2,false,true,true)==NanoRebootHandoff.State.CANCELLED);check(!h.consume());
        h=new NanoRebootHandoff();h.begin(0,true);h.cancel();check(h.poll(1,false,true,true)==NanoRebootHandoff.State.CANCELLED);check(!h.consume());
        boolean[] valid={true,true,true,true};String status="adapter_software_reboot_acknowledged",origin="android-direct-usb";
        check(verified(status,origin,valid,0,0,0));
        for(int i=0;i<valid.length;i++){boolean[] altered=valid.clone();altered[i]=false;check(!verified(status,origin,altered,0,0,0));}
        for(String s:new String[]{"identified","init_handshake_completed","failed","",null})check(!verified(s,origin,valid,0,0,0));
        for(String s:new String[]{"replay","",null})check(!verified(status,s,valid,0,0,0));
        for(int n:new int[]{-1,1,2,999}){check(!verified(status,origin,valid,n,0,0));check(!verified(status,origin,valid,0,n,0));check(!verified(status,origin,valid,0,0,n));}
        System.out.println("NANO_REBOOT_HANDOFF PASS "+checks+" checks; ownership, cleanup, deadline, background/cancel, one launch and bounded adapter-only receipt; no hardware");
    }
}
