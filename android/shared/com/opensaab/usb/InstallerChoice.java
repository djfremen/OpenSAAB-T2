// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Companion-checker routing only; never silently installs or changes an existing app. */
public final class InstallerChoice {
    public static String abi(int api, String[] abis) {
        if (api < 26) return "";
        if (CompatibilityCheck.supportsArm64(abis)) return "arm64-v8a";
        if (CompatibilityCheck.supportsAbi(abis, "armeabi-v7a")) return "armeabi-v7a";
        return "";
    }
    public static String message(int api, String[] abis) {
        String abi = abi(api, abis);
        if (abi.equals("arm64-v8a")) return "Recommended installer: ARM64 Android preview.";
        if (abi.equals("armeabi-v7a")) return "Matching installer: experimental 32-bit head-unit preview. Physical-device and adapter validation is still pending; this is not a compatibility guarantee.";
        return "No matching installer. OpenSAAB needs Android API 26 or newer and ARM64 or ARMv7 application support.";
    }
    public static String url(int api, String[] abis) {
        String abi = abi(api, abis);
        if (abi.equals("arm64-v8a")) return "https://github.com/djfremen/OpenSAAB-T2/releases/tag/v0.1.0-preview.2";
        if (abi.equals("armeabi-v7a")) return "https://github.com/djfremen/OpenSAAB-T2/releases/tag/headunit-v0.1.0-headunit.1";
        return null;
    }
    private InstallerChoice() {}
}
