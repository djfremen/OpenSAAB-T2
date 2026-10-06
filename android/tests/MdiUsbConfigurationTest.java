// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.util.*;

/** Model the kernel's EBUSY rule and verify selection cannot leak interface claims. */
public final class MdiUsbConfigurationTest {
    private static final class Kernel implements MdiUsbConfiguration.Port {
        int active=1,selects,reads,failClaim=-1,failRelease=-1,stopAfterClaim=-1,selectError=0;
        boolean stop,wrongReadback,stopAfterRead,reconnectOnRelease;
        Set<Integer> drivers=new HashSet<>(),owned=new HashSet<>();
        List<String> events=new ArrayList<>();
        public int configuration(){reads++;if(stopAfterRead)stop=true;return wrongReadback&&selects>0?1:active;}
        public int select(int next){
            selects++;events.add("select");
            if(selectError!=0)return selectError;
            if(!drivers.isEmpty()||!owned.isEmpty())return 16;
            active=next;return 0;
        }
        public int[] interfaces(int configuration){return new int[]{0,1,1};}
        public boolean claim(int id){
            events.add("claim"+id);
            if(id==failClaim)return false;
            drivers.remove(id);owned.add(id);if(id==stopAfterClaim)stop=true;return true;
        }
        public int release(int id){
            events.add("release"+id);if(id==failRelease)return 5;
            if(!owned.remove(id))return 22;
            if(reconnectOnRelease)drivers.add(id);
            return 0;
        }
        public boolean stopped(){return stop;}
    }
    private static void require(boolean value,String message){if(!value)throw new AssertionError(message);}
    private static void failure(Kernel k,MdiUsbConfiguration.Trace t,String part)throws Exception{
        try{MdiUsbConfiguration.select(k,2,t);throw new AssertionError("Expected "+part);}
        catch(IOException e){require(e.getMessage().contains(part),e.getMessage());}
    }
    public static void main(String[] args)throws Exception{
        Kernel k=new Kernel();k.active=2;k.drivers.addAll(Arrays.asList(0,1));
        MdiUsbConfiguration.Trace t=new MdiUsbConfiguration.Trace();MdiUsbConfiguration.select(k,2,t);
        require(k.selects==0&&k.owned.isEmpty()&&t.before==2&&t.after==2&&!t.changed,"Already active must not reset a bound configuration");

        k=new Kernel();k.active=2;k.drivers.addAll(Arrays.asList(0,1));
        t=new MdiUsbConfiguration.Trace();MdiUsbConfiguration.select(k,2,t,true);
        require(k.events.equals(Arrays.asList("select","claim0","claim1","release1","release0","select")),"Explicit reinitialization must release kernel/userspace owners before resetting active configuration");
        require(k.owned.isEmpty()&&t.reinitialized&&!t.changed&&t.before==2&&t.after==2&&k.reads==2,"Reinitialization verifies unchanged ID with fresh endpoints and no leaked claims");

        k=new Kernel();k.active=2;t=new MdiUsbConfiguration.Trace();MdiUsbConfiguration.select(k,2,t,true);
        require(k.selects==1&&t.reinitialized&&t.detached==0,"Unbound active configuration resets exactly once");

        k=new Kernel();t=new MdiUsbConfiguration.Trace();MdiUsbConfiguration.select(k,2,t);
        require(k.active==2&&k.selects==1&&t.detached==0&&t.changed&&k.reads==2,"Unbound configuration changes once and is verified");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));t=new MdiUsbConfiguration.Trace();MdiUsbConfiguration.select(k,2,t);
        require(k.events.equals(Arrays.asList("select","claim0","claim1","release1","release0","select")),"Detach all active interfaces and release before changing configuration: "+k.events);
        require(k.owned.isEmpty()&&k.active==2&&t.detached==2&&t.releasesOk,"Configuration switch owns no interfaces");
        require(t.firstError==16&&t.lastError==0&&t.selectionAttempts==2,"Successful retry preserves the original EBUSY");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));k.reconnectOnRelease=true;t=new MdiUsbConfiguration.Trace();failure(k,t,"configuration selection failed");
        require(k.owned.isEmpty()&&k.drivers.size()==2&&k.selects==2&&t.lastError==16,"Android10 release/reconnect reproduces the busy selection failure");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));k.selectError=1;t=new MdiUsbConfiguration.Trace();failure(k,t,"errno 1");
        require(k.events.equals(Arrays.asList("select"))&&k.drivers.size()==2&&t.detached==0,"EPERM must remain a permission error without driver detachment");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));k.failClaim=1;t=new MdiUsbConfiguration.Trace();failure(k,t,"interface unavailable");
        require(k.owned.isEmpty()&&k.selects==1,"Partial claim failure must release the earlier claim and stop");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));k.failRelease=1;t=new MdiUsbConfiguration.Trace();failure(k,t,"release failed");
        require(k.selects==1&&!t.releasesOk&&!k.owned.contains(0),"Failed release prevents selection and still releases other claims");

        k=new Kernel();k.wrongReadback=true;t=new MdiUsbConfiguration.Trace();failure(k,t,"readback mismatch");
        require(t.after==1,"Wrong readback cannot report a ready configuration");

        k=new Kernel();k.stop=true;t=new MdiUsbConfiguration.Trace();failure(k,t,"Stopped");
        require(k.reads==0&&k.events.isEmpty(),"Cancellation before selection sends no control request");

        k=new Kernel();k.active=2;k.stopAfterRead=true;t=new MdiUsbConfiguration.Trace();failure(k,t,"Stopped");
        require(k.events.isEmpty()&&t.after==-1,"Cancellation during readback cannot advance to interface claims");

        k=new Kernel();k.drivers.addAll(Arrays.asList(0,1));k.stopAfterClaim=0;t=new MdiUsbConfiguration.Trace();failure(k,t,"Stopped");
        require(k.owned.isEmpty()&&k.selects==1,"Cancellation during detach releases ownership and prevents retry");
        System.out.println("MDI_USB_CONFIGURATION PASS: active reuse, kernel ownership, readback, partial failures and cancellation");
    }
}
