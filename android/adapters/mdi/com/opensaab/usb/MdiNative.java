// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Packaged MDI transport. No other APK, vendor DLL, root or kernel route. */
final class MdiNative {
    static { System.loadLibrary("opensaab_mdi_android"); }
    static native int nativeRun(int fd,byte[] key,String directory,
        String executable,String firmware,String authority,boolean capture);
    private MdiNative() {}
}
