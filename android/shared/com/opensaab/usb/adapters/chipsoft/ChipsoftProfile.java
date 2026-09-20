// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Owned by the Chipsoft Pro implementation. USB identity is a candidate, not verification. */
public final class ChipsoftProfile {
    public static final AdapterProfile PROFILE=new AdapterProfile(AdapterProfile.Backend.CHIPSOFT_PRO,
        "Chipsoft Pro",0x0483,0x5740,"libchipsoft_probe.so","CDC bulk transport; no CH343 line/control initialization");
    private ChipsoftProfile(){}
}
