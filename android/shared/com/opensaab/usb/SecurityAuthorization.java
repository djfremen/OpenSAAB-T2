// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Activity;
import android.content.Context;

/** Public primary-service compatibility header; not caller authentication. Feature access never asks the operator for a password. */
public final class SecurityAuthorization {
    public static String bearer(Context context){return "OpenSAAB";}
    public static boolean available(Context context){return true;}
    public static void show(Activity activity){show(activity,()->{});}
    public static void show(Activity activity,Runnable ready){ready.run();}
    public static void forget(Context context){} // Historical callers; stored credentials are never read.
    private SecurityAuthorization(){}
}
