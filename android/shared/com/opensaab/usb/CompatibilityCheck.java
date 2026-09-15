// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Requirements for the published ARM64 emulator; independent of CPU marketing names. */
public final class CompatibilityCheck {
    public static final long SETUP_BYTES = 140L * 1024 * 1024;
    public static boolean supportsArm64(String[] abis) {
        if (abis != null) for (String abi : abis) if ("arm64-v8a".equals(abi)) return true;
        return false;
    }
    public static boolean platformSupported(int api, String[] abis) {
        return api >= 26 && supportsArm64(abis);
    }
    public static String verdict(int api, String[] abis, long freeBytes) {
        if (!platformSupported(api, abis)) return "Current OpenSAAB APK is not compatible";
        if (freeBytes < SETUP_BYTES) return "Platform supported — free up storage before setup";
        return "Basic installation requirements met";
    }
    private CompatibilityCheck() {}
}
