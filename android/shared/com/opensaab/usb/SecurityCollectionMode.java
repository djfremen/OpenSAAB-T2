// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import java.util.function.Consumer;

/** An explicit collection choice; a missing choice always leaves navigation to the owner. */
public final class SecurityCollectionMode {
    public static final String EXTRA="security_collection_mode", MANUAL="manual", GUIDED="guided";
    public static final String PLATFORM_EXTRA="security_collection_platform";
    public static String normalize(String value){return GUIDED.equals(value)?GUIDED:MANUAL;}
    public static String from(Intent intent){return normalize(intent.getStringExtra(EXTRA));}
    public static void choose(Activity activity,Consumer<String> start){
        new AlertDialog.Builder(activity).setTitle("Collect fresh security data")
            .setMessage("Manual: you open Diagnostics → All → Get Security Access.\n\nGuided: for Saab 9-3 Sport (9440), model years 2003–2012. Choosing Guided confirms this vehicle model. OpenSAAB selects those menus; the original firmware collects the seeds. You handle every ignition-key prompt.\n\nCollection works locally. Previous security data is kept until a new response is verified. Internet and the password are needed only when you choose Process security data.")
            .setNegativeButton("Cancel",null)
            .setNeutralButton("Manual collection",(dialog,which)->{activity.getIntent().removeExtra(PLATFORM_EXTRA);start.accept(MANUAL);})
            .setPositiveButton("Guided collection",(dialog,which)->{activity.getIntent().putExtra(PLATFORM_EXTRA,"9440");start.accept(GUIDED);}).show();
    }
    public static VehicleIdentity navigationIdentity(Activity activity,VehicleIdentity vehicle){
        if(vehicle==null||!GUIDED.equals(from(activity.getIntent()))||!"9440".equals(activity.getIntent().getStringExtra(PLATFORM_EXTRA))||vehicle.modelYear<2003||vehicle.modelYear>2012||!vehicle.platform.isEmpty())return vehicle;
        // Local operator-confirmed menu target; do not claim an online vehicle lookup.
        return new VehicleIdentity(vehicle.vin,vehicle.observedUtc,vehicle.source,vehicle.modelYear,vehicle.lookupStatus,"9440",vehicle.engine,vehicle.color,vehicle.gearbox,vehicle.body);
    }
    public static void record(Activity activity,java.io.File run)throws java.io.IOException{
        String mode=from(activity.getIntent());
        java.nio.file.Files.write(new java.io.File(run,"collection-mode.txt").toPath(),mode.getBytes(java.nio.charset.StandardCharsets.US_ASCII),java.nio.file.StandardOpenOption.CREATE_NEW);
        android.util.Log.i("OpenSaabSecurity","Fresh collection started mode="+mode+" source_preserved=true commands=original-firmware");
    }
    private SecurityCollectionMode(){}
}
