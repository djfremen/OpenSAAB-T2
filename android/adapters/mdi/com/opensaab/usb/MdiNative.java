// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Packaged MDI transport. No other APK, vendor DLL, root or kernel route. */
final class MdiNative {
    static { System.loadLibrary("opensaab_mdi_android"); }
    /** Zero on success, positive errno on failure. These calls borrow the granted fd. */
    static native int nativeSelectConfiguration(int fd,int configuration);
    static native int nativeReleaseForConfiguration(int fd,int interfaceId);
    static native int nativeRun(int fd,String directory,
        String executable,String firmware,String authority,boolean capture);
    private MdiNative() {}
}
