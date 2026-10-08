// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Compare the native binding with independently loaded common-contract values. */
public final class FirmwareScrollContractTest {
    public static void main(String[] expected) {
        String[] actual = {
            FirmwareScrollPreferences.KEY, FirmwareScrollPreferences.LABEL,
            String.valueOf(FirmwareScrollPreferences.DEFAULT_NATURAL),
            String.valueOf(FirmwareScrollPreferences.swipeKey(true, false)),
            String.valueOf(FirmwareScrollPreferences.swipeKey(false, false)),
            String.valueOf(FirmwareScrollPreferences.swipeKey(true, true)),
            String.valueOf(FirmwareScrollPreferences.swipeKey(false, true)),
            FirmwareScrollPreferences.description(false), FirmwareScrollPreferences.description(true)
        };
        if (!java.util.Arrays.equals(actual, expected))
            throw new AssertionError("Android gesture preference differs from the common contract");
        System.out.println("PASS: shared preference key, label, default, four mappings and both descriptions");
    }
}
