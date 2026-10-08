// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Regression: a USB identity cannot route to a different adapter's command owner. */
public final class AdapterRoutingTest {
    static void check(boolean ok){if(!ok)throw new AssertionError();}
    public static void main(String[] args){
        AdapterCatalog.Match chip=AdapterCatalog.identify(0x0483,0x5740);
        AdapterCatalog.Match nano=AdapterCatalog.identify(0x1a86,0x55d3);
        check(chip.backend()==AdapterProfile.Backend.CHIPSOFT_PRO);
        check(nano.backend()==AdapterProfile.Backend.VCX_NANO);
        check(chip.profile.probeExecutable.equals("libchipsoft_probe.so"));
        check(nano.profile.probeExecutable.equals("libnano_probe.so"));
        check(!chip.profile.matches(0x1a86,0x55d3));
        check(!nano.profile.matches(0x0483,0x5740));
        check(chip.profile.candidateLabel().contains("candidate"));
        check(nano.profile.openingMessage().contains("checking identity"));
        for(int[] ids:new int[][]{{0x0483,0x5741},{0x1a86,0x7523},{0x0ca0,0x1201},{0x0525,0xa4a2},{0x1234,0x5678}}){
            AdapterCatalog.Match m=AdapterCatalog.identify(ids[0],ids[1]);
            check(m.backend()==AdapterProfile.Backend.NONE);
            check(m.profile==null && !AdapterCatalog.supported(m));
        }
        System.out.println("ADAPTER_ROUTING PASS; distinct drivers; unsupported IDs fail closed; identity remains unverified");
    }
}
