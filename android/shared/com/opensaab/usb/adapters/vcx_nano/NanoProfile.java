// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Owned by the VCX Nano implementation. USB identity is a candidate, not verification. */
public final class NanoProfile {
    public static final AdapterProfile PROFILE=new AdapterProfile(AdapterProfile.Backend.VCX_NANO,
        "VCX Nano",0x1a86,0x55d3,"libnano_probe.so","Captured CH343 control initialization; Nano protocol handshake");
    private NanoProfile(){}
}
