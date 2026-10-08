// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
/** Fast ARMv7 startup; ARM64 keeps its existing startup behavior. */
public final class DemandStartup {
    public static boolean enabled(android.content.Context context) {
        return context.getPackageName().equals("com.opensaab.tech2.headunit32")
            || (context.getPackageName().equals("com.opensaab.tech2.headunit32.test")
            && (context.getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)!=0);
    }
    public static void configure(android.content.Context context,ProcessBuilder builder) {
        if(!enabled(context) || !builder.command().contains("--candi-native-link"))return;
        builder.command().add("--candi-on-demand");
        builder.environment().put("LOAD_TEST_ACCELERATE","1");
    }
    /** The emulator may remain in menus for minutes before opening its backend. */
    public static java.net.Socket accept(android.content.Context context,java.net.ServerSocket server,
            ProcessBuilder builder,java.lang.Process child,java.util.function.BooleanSupplier cancelled) throws java.io.IOException {
        if(!enabled(context) || !builder.command().contains("--candi-on-demand"))return server.accept();
        server.setSoTimeout(1000);
        long deadline=android.os.SystemClock.elapsedRealtime()+30*60*1000;
        while(!cancelled.getAsBoolean() && child.isAlive() && android.os.SystemClock.elapsedRealtime()<deadline) {
            try{return server.accept();}catch(java.net.SocketTimeoutException waiting){}
        }
        throw new java.io.IOException("Deferred adapter wait ended: stopped, emulator exited, or session deadline reached");
    }
}
