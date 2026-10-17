// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.os.Build;
import android.os.Bundle;

/** Test-only guard, independent of the production debug simulator transport. */
final class DisposableEmulator {
    static void require(Context context) {
        boolean model = Build.MODEL.startsWith("sdk_gphone") || Build.PRODUCT.startsWith("sdk_gphone");
        boolean hardware = Build.HARDWARE.equals("ranchu") || Build.HARDWARE.equals("goldfish");
        if (!model || !hardware) throw new AssertionError("Disposable Google Android emulator required; never run on a physical device");
    }
    static String requireInstalledVersion(Context context, Bundle arguments) throws Exception {
        String expected = arguments == null ? null : arguments.getString("expected_version");
        String actual = context.getPackageManager().getPackageInfo(context.getPackageName(),0).versionName;
        if (expected == null || expected.isEmpty() || !expected.equals(actual))
            throw new AssertionError("Supply -e expected_version with the exact installed candidate version: " + actual);
        return actual;
    }
    private DisposableEmulator() {}
}
