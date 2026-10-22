// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.util.function.Consumer;
import java.util.function.LongSupplier;

/** One stopped run's transport; no image lease or ECU authority is retained. */
final class SecurityTransportLease<T> implements AutoCloseable {
    private final String run,card,device,vin,beforeHash;
    private final boolean compatibility;
    private final long expires;
    private final LongSupplier clock;
    private final Consumer<T> release;
    private T transport;
    private String importedHash;
    SecurityTransportLease(String run,String card,String device,boolean compatibility,String vin,String beforeHash,
            long expires,LongSupplier clock,T transport,Consumer<T> release,
            boolean processDead,boolean cleanExit,boolean protocolClean)throws IOException{
        if(run==null||run.isEmpty()||card==null||card.isEmpty()||device==null||device.isEmpty()||vin==null||vin.length()!=17
                ||beforeHash==null||!beforeHash.matches("[a-f0-9]{64}")||clock==null||transport==null||release==null){
            if(transport!=null&&release!=null)release.accept(transport);
            throw new IOException("Incomplete security transport binding");
        }
        this.run=run;this.card=card;this.device=device;this.compatibility=compatibility;this.vin=vin;this.beforeHash=beforeHash;
        this.expires=expires;this.clock=clock;this.transport=transport;this.release=release;
        if(!processDead||!cleanExit||!protocolClean||clock.getAsLong()>=expires){close();throw new IOException("Native shutdown did not qualify for transport retention");}
    }
    synchronized boolean available(){return transport!=null&&clock.getAsLong()<expires;}
    void imported(String run,String card,String beforeHash,String afterHash)throws IOException{
        boolean valid;
        synchronized(this){valid=available()&&this.run.equals(run)&&this.card.equals(card)&&this.beforeHash.equals(beforeHash)
                &&afterHash!=null&&afterHash.matches("[a-f0-9]{64}")&&!beforeHash.equals(afterHash)&&importedHash==null;
            if(valid)importedHash=afterHash;}
        if(!valid){close();throw new IOException("Security import no longer matches the retained run/card");}
    }
    T claim(String run,String card,String device,boolean compatibility,String vin,String currentHash)throws IOException{
        T result=null;
        synchronized(this){if(available()&&this.run.equals(run)&&this.card.equals(card)&&this.device.equals(device)
                &&this.compatibility==compatibility&&this.vin.equals(vin)&&importedHash!=null&&importedHash.equals(currentHash)){
            result=transport;transport=null;}}
        if(result==null){close();throw new IOException("Retained connection expired or its run/card/vehicle changed");}
        return result;
    }
    public void close(){T value;synchronized(this){value=transport;transport=null;}if(value!=null)release.accept(value);}
}
