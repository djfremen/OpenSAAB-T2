// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class AdapterCatalogTest {
    static void expect(int v,int p,String family){if(!AdapterCatalog.identify(v,p).family.equals(family))throw new AssertionError(Integer.toHexString(v)+":"+Integer.toHexString(p));}
    public static void main(String[] args){
        if(args.length==1 && args[0].equals("--bosch-ids")){
            for(int vendor:new int[]{0x0ca0,0x05a0})for(int product=0;product<=65535;product++)if(AdapterCatalog.identify(vendor,product).family.equals("bosch_etas_vci_candidate"))System.out.printf("%04X:%04X%n",vendor,product);return;
        }
        expect(0x1a86,0x55d3,"nano_candidate");expect(0x1a86,0x7523,"unknown");
        expect(0x0483,0x5740,"chipsoft_candidate");expect(0x0483,0x5741,"unknown");
        expect(0x0ca0,0x1201,"bosch_etas_vci_candidate");expect(0x0ca0,0x14ff,"bosch_etas_vci_candidate");
        expect(0x0ca0,0x1200,"unknown");expect(0x0ca0,0x1310,"unknown");expect(0x0ca0,0x14f0,"unknown");
        expect(0x05a0,0x0100,"bosch_etas_vci_candidate");expect(0x05a0,0x0300,"bosch_etas_vci_candidate");expect(0x05a0,0x03ff,"bosch_etas_vci_candidate");expect(0x05a0,0x0400,"unknown");
        expect(0x0525,0xa4a2,"generic_rndis");expect(0x0ca0,0x1501,"bosch_eps_candidate");expect(0x0ca0,0x1504,"unknown");
        expect(0x0bda,0x8153,"unknown");expect(0x05e3,0x0610,"unknown");
        System.out.println("ADAPTER_CATALOG PASS; metadata only, no hardware access");
    }
}
