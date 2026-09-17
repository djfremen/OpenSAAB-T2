// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.util.Locale;

/** Display-only history: connection time is the saved VIN observation, never app-open time. */
public final class VehicleHistoryStatus {
    public final String connection,auth,timestamp;
    public final int color;
    public VehicleHistoryStatus(VehicleIdentity vehicle,SecurityAccessStatus receipt,File card,Instant now){
        connection="Last connection · "+time(vehicle==null?null:vehicle.observedUtc);
        String label="auth_status: [N/A]",when="Timestamp unavailable";int tint=SsaState.UNAVAILABLE.color;
        if(vehicle!=null){
            try(RandomAccessFile f=new RandomAccessFile(card,"r")){
                byte[] bytes=new byte[SsaData.SIZE];f.seek(SsaData.OFFSET);f.readFully(bytes);
                SsaState state=SsaState.analyze(bytes);
                // A cleared card has no vehicle association. Do not borrow another vehicle's receipt.
                if(state==SsaState.INIT_AUTH){label="auth_status: [INIT_AUTH] · cleared";tint=state.color;}
                else if(vehicle.vin.equals(SsaData.vin(bytes))){
                    label="auth_status: "+state.label;tint=state.color;
                    if(receipt!=null&&receipt.matches(vehicle.vin)){
                        if(state==SsaState.POST_AUTH&&receipt.cardMatches(card)){
                            String age=receipt.freshness(true,now);label+=" · "+age;
                            when="Post-auth written · "+time(receipt.data.getProperty("imported_utc"));
                            if("Stale".equals(age))tint=0xffffd77c;
                        }else if(state==SsaState.PRE_AUTH&&!receipt.imported()){
                            when="Pre-auth collected · "+time(receipt.data.getProperty("pre_auth_utc"));
                        }
                    }
                }
            }catch(Exception unavailable){/* Keep unknown rather than attributing a different card to this vehicle. */}
        }
        auth=label;timestamp=when;color=tint;
    }
    public static String time(String utc){
        try{return DateTimeFormatter.ofPattern("MMM d, yyyy · HH:mm:ss z",Locale.getDefault()).withZone(ZoneId.systemDefault()).format(Instant.parse(utc));}
        catch(Exception missing){return "Date unknown";}
    }
}
