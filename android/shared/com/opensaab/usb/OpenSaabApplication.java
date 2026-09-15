// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
/** Save bounded crash metadata, then preserve Android's normal crash handling. */
public final class OpenSaabApplication extends android.app.Application {
    public void onCreate(){super.onCreate();Thread.UncaughtExceptionHandler previous=Thread.getDefaultUncaughtExceptionHandler();Thread.setDefaultUncaughtExceptionHandler((thread,error)->{SupportReports.recordError(this,error,true);if(previous!=null)previous.uncaughtException(thread,error);else{android.os.Process.killProcess(android.os.Process.myPid());System.exit(1);}});}
}
