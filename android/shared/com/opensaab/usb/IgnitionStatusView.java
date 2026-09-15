// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.widget.TextView;
import java.io.File;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** Separate from the original LCD: vehicle telemetry never changes guest pixels. */
public final class IgnitionStatusView extends TextView {
    public IgnitionStatusView(Context context) {
        super(context);setTextSize(15);setText("Ignition: Unknown · no CAN observation");
    }
    public void refresh(File directory,boolean active) {
        if(!active){setText("Ignition: Unknown · disconnected");return;}
        if(directory==null){setText("Ignition: Unknown · awaiting CIM reply");return;}
        try {
            File f=new File(directory,"ignition-status.json");
            if(f.length()<1 || f.length()>4096)throw new Exception("Missing telemetry");
            JSONObject v=new JSONObject(new String(java.nio.file.Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8));
            if(v.getInt("schema")!=1 || !(v.getString("profile").equals("saab-9440-cim-capture-validated") || v.getString("profile").equals("saab-9440-1367-capture-validated")))throw new Exception("Unknown profile");
            setText(IgnitionStatusText.format(v.getString("state"),v.getString("freshness"),v.getString("last_observed_state"),
                v.isNull("age_ms")?-1:v.getLong("age_ms"),System.currentTimeMillis()-v.getLong("generated_unix_ms"),active,v.getBoolean("connected")));
        } catch(Exception e) {setText("Ignition: Unknown · awaiting CIM telemetry");}
    }
}
