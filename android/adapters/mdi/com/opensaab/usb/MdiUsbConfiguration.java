// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashSet;

/** Configuration selection only; no RNDIS, management or vehicle commands. */
final class MdiUsbConfiguration {
    interface Port {
        int configuration() throws IOException;
        int select(int configuration);
        int[] interfaces(int configuration) throws IOException;
        boolean claim(int interfaceId);
        int release(int interfaceId);
        boolean stopped();
    }
    static final class Trace {
        int before=-1,after=-1,detached=0;
        int selectionAttempts=0,firstError=0,lastError=0,releaseError=0;
        boolean changed=false,reinitialized=false,releasesOk=true;
    }
    private static void check(Port port)throws IOException {
        if(port.stopped())throw new IOException("Stopped before MDI configuration selection");
    }
    private static boolean trySelect(Port port,int required,Trace trace){
        int error=port.select(required);trace.selectionAttempts++;trace.lastError=error;
        if(trace.selectionAttempts==1)trace.firstError=error;
        return error==0;
    }
    private static IOException failed(int required,Trace trace){
        return new IOException("MDI configuration selection failed (active "+trace.before+", requested "+required+", errno "+trace.lastError+")");
    }
    static void select(Port port,int required,Trace trace)throws IOException {
        select(port,required,trace,false);
    }
    static void select(Port port,int required,Trace trace,boolean reinitialize)throws IOException {
        check(port);trace.before=port.configuration();check(port);
        // Linux rejects SET_CONFIGURATION while a kernel driver owns an interface,
        // even when selecting the already-active configuration.
        if(trace.before==required&&!reinitialize){trace.after=trace.before;return;}
        check(port);
        if(!trySelect(port,required,trace)){
            // Only EBUSY indicates interface ownership. Do not detach drivers
            // after a permission denial, stall, disconnect or another I/O error.
            if(trace.before==0||trace.lastError!=16)throw failed(required,trace);
            ArrayList<Integer> owned=new ArrayList<>();
            try{
                LinkedHashSet<Integer> ids=new LinkedHashSet<>();
                for(int id:port.interfaces(trace.before))ids.add(id);
                if(ids.isEmpty())throw new IOException("MDI active USB interfaces unavailable");
                for(int id:ids){
                    check(port);
                    if(!port.claim(id))throw new IOException("MDI active USB interface unavailable");
                    owned.add(id);trace.detached++;
                }
            }finally{
                // No userspace claim may remain during SET_CONFIGURATION.
                for(int n=owned.size()-1;n>=0;n--){int error=port.release(owned.get(n));trace.releasesOk&=error==0;if(error!=0)trace.releaseError=error;}
            }
            if(!trace.releasesOk)throw new IOException("MDI active USB interface release failed (errno "+trace.releaseError+")");
            check(port);
            if(!trySelect(port,required,trace))throw failed(required,trace);
        }
        trace.changed=trace.before!=required;trace.reinitialized=trace.before==required;
        check(port);trace.after=port.configuration();
        if(trace.after!=required)throw new IOException("MDI USB configuration readback mismatch");
    }
}
