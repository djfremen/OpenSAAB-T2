// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;

/** Installed APK identity, never inferred from the device's CPU marketing name. */
public final class AppBuildProfile {
    public static boolean isHeadunit32(Context context) {
        return "com.opensaab.tech2.headunit32".equals(context.getPackageName());
    }
    public static String abi(Context context) {
        return isHeadunit32(context) ? "armeabi-v7a" : "arm64-v8a";
    }
    public static String name(Context context) {
        return isHeadunit32(context) ? "headunit-arm32-development" : "arm64";
    }
    private AppBuildProfile() {}
}
