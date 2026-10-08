// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
/** Save bounded crash metadata, then preserve Android's normal crash handling. */
public final class OpenSaabApplication extends android.app.Application {
    public void onCreate(){super.onCreate();registerActivityLifecycleCallbacks(new ActivityLifecycleCallbacks(){
        public void onActivityResumed(android.app.Activity a){ProjectSupport.attachFooter(a);EmulatorHealthMonitor.offerPreviousCrash(a);}
        public void onActivityCreated(android.app.Activity a,android.os.Bundle b){} public void onActivityStarted(android.app.Activity a){} public void onActivityPaused(android.app.Activity a){} public void onActivityStopped(android.app.Activity a){} public void onActivitySaveInstanceState(android.app.Activity a,android.os.Bundle b){} public void onActivityDestroyed(android.app.Activity a){}
    });Thread.UncaughtExceptionHandler previous=Thread.getDefaultUncaughtExceptionHandler();Thread.setDefaultUncaughtExceptionHandler((thread,error)->{SupportReports.recordError(this,error,true);if(previous!=null)previous.uncaughtException(thread,error);else{android.os.Process.killProcess(android.os.Process.myPid());System.exit(1);}});}
}
