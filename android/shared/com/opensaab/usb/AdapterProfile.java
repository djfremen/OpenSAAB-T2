// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Immutable routing metadata only. Construction never opens USB or transmits. */
public final class AdapterProfile {
    public enum Backend { NONE, CHIPSOFT_PRO, VCX_NANO }
    public final Backend backend;
    public final String name, probeExecutable, initialization;
    public final int vendorId, productId;
    public AdapterProfile(Backend backend,String name,int vendor,int product,String probe,String initialization){
        this.backend=backend;this.name=name;vendorId=vendor;productId=product;
        probeExecutable=probe;this.initialization=initialization;
    }
    public boolean matches(int vendor,int product){return vendor==vendorId && product==productId;}
    public String candidateLabel(){return name+" candidate";}
    public String openingMessage(){return name+" USB candidate — opening its driver and checking identity…";}
}
