// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** USB identity hints only. No device I/O, authentication, or capability proof. */
public final class AdapterCatalog {
    public static final class Match {
        public final String family,label,transport,readiness,evidence;
        public final AdapterProfile profile;
        public AdapterProfile.Backend backend(){return profile==null?AdapterProfile.Backend.NONE:profile.backend;}
        Match(String f,String l,String t,String r,String e){this(f,l,t,r,e,null);}
        Match(String f,String l,String t,String r,String e,AdapterProfile p){family=f;label=l;transport=t;readiness=r;evidence=e;profile=p;}
    }
    public static Match identify(int vendor,int product){
        if(NanoProfile.PROFILE.matches(vendor,product))return new Match("nano_candidate","VCX Nano candidate","CH343 serial USB",
            "Nano backend available; verify adapter identity before use","Captured Nano USB identity; shared USB chip IDs are not firmware identification",NanoProfile.PROFILE);
        if(ChipsoftProfile.PROFILE.matches(vendor,product))return new Match("chipsoft_candidate","Chipsoft Pro candidate","USB CDC serial",
            "Android identity, raw receive and original firmware read backend available; verify identity before use","Chipsoft capture matches; this STM USB identity is also used by unrelated devices",ChipsoftProfile.PROFILE);
        // Active entries only in Bosch MDI/MDI2 boschvci_v3.inf. Low-nibble-zero
        // IDs are commented out in the 0CA0 12xx/13xx/14xx ranges.
        if((vendor==0x0ca0 && product>=0x1201 && product<=0x14ff && (product&15)!=0)
            || (vendor==0x05a0 && product>=0x0300 && product<=0x03ff)
            || (vendor==0x05a0 && product==0x0100))
            return new Match("bosch_etas_vci_candidate","Bosch / ETAS VCI candidate · possible MDI family","USB networking (RNDIS)",
                "Detection only; Android MDI backend pending","Official MDI and MDI 2 driver ID match; exact model and firmware unverified");
        if(vendor==0x0ca0 && product>=0x1501 && product<=0x153f && (product&3)!=0)
            return new Match("bosch_eps_candidate","Bosch EPS candidate","USB networking (RNDIS)",
                "No supported diagnostic backend","Bosch INF identifies EPS separately; do not classify as MDI");
        if(vendor==0x0525 && product==0xa4a2)return new Match("generic_rndis","Generic USB network device","RNDIS candidate",
            "Adapter identity unknown","Generic Linux USB ID appears in ETAS INF; insufficient to identify MDI");
        return new Match("unknown","Unrecognized USB device","Inspect interfaces",
            "No backend selected","No matching adapter profile; vendor/product names are informational only");
    }
    public static boolean adapterCandidate(Match match){return match.family.endsWith("_candidate") && !match.family.equals("bosch_eps_candidate");}
    public static boolean supported(Match match){return match.backend()!=AdapterProfile.Backend.NONE;}
    private AdapterCatalog(){}
}
