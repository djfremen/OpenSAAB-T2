// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.IOException;
import java.util.concurrent.atomic.AtomicLong;

public final class SecurityTransportLeaseTest {
    interface Attempt {void run()throws Exception;}
    static final String BEFORE="a".repeat(64),AFTER="b".repeat(64),VIN="YS3FD79Y276000001";
    static final class Link {int closes;}
    static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    static void rejected(Attempt action)throws Exception{try{action.run();throw new AssertionError("Unproven security handoff accepted");}catch(IOException expected){}}
    static SecurityTransportLease<Link> lease(Link link,AtomicLong now)throws Exception{
        return new SecurityTransportLease<>("run","card","adapter",false,VIN,BEFORE,1000,now::get,link,x->x.closes++,true,true,true);
    }
    public static void main(String[] args)throws Exception{
        AtomicLong now=new AtomicLong(100);
        for(int proof=0;proof<3;proof++){
            Link link=new Link();final int missing=proof;
            rejected(()->new SecurityTransportLease<>("run","card","adapter",false,VIN,BEFORE,1000,now::get,link,x->x.closes++,missing!=0,missing!=1,missing!=2));
            check(link.closes==1,"Live/failed firmware proof leaked transport");
        }
        Link invalid=new Link();rejected(()->new SecurityTransportLease<>("","card","adapter",false,VIN,BEFORE,1000,now::get,invalid,x->x.closes++,true,true,true));
        check(invalid.closes==1,"Malformed binding leaked transport");
        for(int scenario=0;scenario<12;scenario++){
            now.set(100);Link link=new Link();SecurityTransportLease<Link> owner=lease(link,now);
            final int test=scenario;
            if(scenario<5){
                if(scenario==4)now.set(1000);
                rejected(()->owner.imported(test==0?"other":"run",test==1?"other":"card",test==2?AFTER:BEFORE,test==3?BEFORE:AFTER));
            }else{
                owner.imported("run","card",BEFORE,AFTER);
                if(scenario==5)rejected(()->owner.imported("run","card",BEFORE,AFTER));
                else{
                    if(scenario==11)now.set(1000);
                    rejected(()->owner.claim(test==6?"other":"run",test==7?"other":"card",test==8?"other":"adapter",test==9,test==10?"YS3FD79Y276000002":VIN,AFTER));
                }
            }
            owner.close();check(link.closes==1,"Rejected import/claim must close exactly once");
        }
        now.set(100);Link changed=new Link();SecurityTransportLease<Link> changedOwner=lease(changed,now);
        changedOwner.imported("run","card",BEFORE,AFTER);rejected(()->changedOwner.claim("run","card","adapter",false,VIN,BEFORE));check(changed.closes==1,"Post-import mutation accepted");
        Link unimported=new Link();SecurityTransportLease<Link> unimportedOwner=lease(unimported,now);rejected(()->unimportedOwner.claim("run","card","adapter",false,VIN,BEFORE));check(unimported.closes==1,"Missing import accepted");
        Link retained=new Link();SecurityTransportLease<Link> owner=lease(retained,now);
        // A transport lease does not retain the firmware/card lease after actual child death.
        try(FirmwareGate.Lease image=FirmwareGate.change()){
            check(owner.available(),"Card mutation unnecessarily closed the proven transport");owner.imported("run","card",BEFORE,AFTER);
        }
        check(owner.claim("run","card","adapter",false,VIN,AFTER)==retained,"Handoff replaced original socket");
        owner.close();rejected(()->owner.claim("run","card","adapter",false,VIN,AFTER));check(retained.closes==0,"Consumed lease closed its new owner's socket");
        Link cancel=new Link();SecurityTransportLease<Link> cancelled=lease(cancel,now);cancelled.close();cancelled.close();check(cancel.closes==1&&!cancelled.available(),"Cancellation leaked or duplicated close");
        System.out.println("PASS: stopped-child proofs, bounded run/card/device/mode/VIN/import binding, one-use ownership, cancellation and independent card mutation");
    }
}
