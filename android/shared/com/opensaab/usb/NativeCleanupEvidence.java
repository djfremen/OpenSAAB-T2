// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.File;

/** Read only after the native worker has exited; absent evidence stays unknown. */
public final class NativeCleanupEvidence {
    private NativeCleanupEvidence(){}
    public static String protocol(File run){
        try{
            String value=FirmwareStore.json(new File(run,"adapter-diagnostics.json")).optString("protocol_cleanup");
            if(value.equals("succeeded")||value.equals("failed"))return value;
        }catch(Exception unavailable){}
        return "unknown";
    }
}
