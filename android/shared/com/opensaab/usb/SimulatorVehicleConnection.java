// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.content.Context;
import android.os.Build;
import java.io.File;
/** Debug emulator transport facade; physical devices use VlinkerVehicleConnection.direct. */
public final class SimulatorVehicleConnection extends VlinkerVehicleConnection {
    public SimulatorVehicleConnection(Context c,Listener l){super(c,l,()->new VlinkerLoopbackTransport("10.0.2.2",8767),"android-emulator-local-m4-bluetooth",available(c));}
    SimulatorVehicleConnection(Context c,Listener l,int fixturePort){super(c,l,()->new VlinkerLoopbackTransport("127.0.0.1",fixturePort),"synthetic-fixture",available(c));if(!available(c)||fixturePort<1024||fixturePort>65535)throw new IllegalArgumentException("Debug emulator fixture required");}
    public static boolean available(Context c){
        boolean debug=(c.getApplicationInfo().flags&android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)!=0;
        boolean emulator=Build.FINGERPRINT.startsWith("generic")||Build.FINGERPRINT.contains("emulator")||Build.HARDWARE.equals("ranchu")||Build.HARDWARE.equals("goldfish");
        return debug&&emulator&&VlinkerVehicleConnection.available(c);
    }
}
