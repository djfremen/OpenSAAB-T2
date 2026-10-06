// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.File;
import android.content.Context;

/** USB candidate routing and installed payload availability. Identity comes from the adapter. */
public final class MdiAdapter {
    private MdiAdapter() {}
    public static boolean candidate(AdapterCatalog.Match match){return match.family.equals("generic_rndis")||match.family.equals("bosch_etas_vci_candidate");}
    public static boolean packaged(Context c){return new File(c.getApplicationInfo().nativeLibraryDir,"libopensaab_mdi_android.so").isFile()
        &&new File(c.getApplicationInfo().nativeLibraryDir,"libtech2_mdi.so").isFile();}
}
