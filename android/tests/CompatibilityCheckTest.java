// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class CompatibilityCheckTest {
    private static void require(boolean ok) { if (!ok) throw new AssertionError(); }
    public static void main(String[] args) {
        require(!CompatibilityCheck.platformSupported(31,new String[]{"armeabi-v7a","armeabi"}));
        require(!CompatibilityCheck.platformSupported(25,new String[]{"arm64-v8a"}));
        require(CompatibilityCheck.platformSupported(26,new String[]{"arm64-v8a","armeabi-v7a"}));
        require(!CompatibilityCheck.supportsArm64(new String[]{"x86_64"}));
        require(!CompatibilityCheck.supportsArm64(null));
        require(CompatibilityCheck.verdict(29,new String[]{"armeabi-v7a","armeabi"},CompatibilityCheck.SETUP_BYTES,"armeabi-v7a").equals("Basic installation requirements met"));
        require(CompatibilityCheck.verdict(29,new String[]{"arm64-v8a"},CompatibilityCheck.SETUP_BYTES,"armeabi-v7a").contains("not compatible"));
        require(CompatibilityCheck.verdict(25,new String[]{"armeabi-v7a"},CompatibilityCheck.SETUP_BYTES,"armeabi-v7a").contains("not compatible"));
        require(CompatibilityCheck.verdict(26,new String[]{"arm64-v8a"},CompatibilityCheck.SETUP_BYTES-1).contains("free up storage"));
        require(CompatibilityCheck.verdict(26,new String[]{"arm64-v8a"},CompatibilityCheck.SETUP_BYTES).equals("Basic installation requirements met"));
        System.out.println("PASS: ARM32, ARM64, x86_64, API and storage compatibility boundaries");
    }
}
