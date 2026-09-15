// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.os.Build;
import android.window.OnBackInvokedDispatcher;

/** Preserve the same cancellation/EXIT behavior for modern Android back gestures. */
public final class BackNavigation {
    public static void install(Activity activity, Runnable action) {
        if (Build.VERSION.SDK_INT >= 33) {
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                OnBackInvokedDispatcher.PRIORITY_DEFAULT, action::run);
        }
    }
    private BackNavigation() {}
}
