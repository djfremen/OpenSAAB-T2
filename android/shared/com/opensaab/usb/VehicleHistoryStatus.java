// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.util.Locale;

/** Display-only history: connection time is the saved VIN observation, never app-open time. */
public final class VehicleHistoryStatus {
    public final String connection,auth,timestamp;
    public final String authState,freshness,statusUtc;
    public final int color;
    public VehicleHistoryStatus(VehicleIdentity vehicle,SecurityAccessStatus receipt,File card,Instant now){this(vehicle,receipt,readCard(card),now,true);}
    public static VehicleHistoryStatus working(VehicleIdentity vehicle,SecurityAccessStatus receipt,byte[] bytes,Instant now){return new VehicleHistoryStatus(vehicle,receipt,bytes,now,true);}
    private static byte[] readCard(File card){try(RandomAccessFile f=new RandomAccessFile(card,"r")){byte[] b=new byte[SsaData.SIZE];f.seek(SsaData.OFFSET);f.readFully(b);return b;}catch(Exception unavailable){return null;}}
    private VehicleHistoryStatus(VehicleIdentity vehicle,SecurityAccessStatus receipt,byte[] bytes,Instant now,boolean selected){
        connection="Last connection · "+time(vehicle==null?null:vehicle.observedUtc);
        String label="auth_status: [N/A]",when="Timestamp unavailable";
        String stateLabel=SsaState.UNAVAILABLE.label,age="Age unknown",utc=null;int tint=SsaState.UNAVAILABLE.color;
        if(vehicle!=null){
            try{
                SsaState state=SsaState.analyze(bytes);
                // A cleared card has no vehicle association. Do not borrow another vehicle's receipt.
                if(state==SsaState.INIT_AUTH){stateLabel=state.label;label="auth_status: [INIT_AUTH] · cleared";tint=state.color;}
                else if(vehicle.vin.equals(SsaData.vin(bytes))){
                    stateLabel=state.label;label="auth_status: "+state.label;tint=state.color;
                    if(receipt!=null&&receipt.matches(vehicle.vin)){
                        if(state==SsaState.POST_AUTH&&receipt.ssaMatches(bytes)){
                            age=receipt.freshness(true,now);label+=" · "+age;
                            utc=receipt.data.getProperty("imported_utc");when="Post-auth written · "+time(utc);
                            if("Stale".equals(age))tint=0xffffd77c;
                        }else if(state==SsaState.PRE_AUTH&&!receipt.imported()){
                            utc=receipt.data.getProperty("pre_auth_utc");when="Pre-auth collected · "+time(utc);
                        }
                    }
                }
            }catch(Exception unavailable){/* Keep unknown rather than attributing a different card to this vehicle. */}
        }
        auth=label;timestamp=when;color=tint;authState=stateLabel;freshness=age;statusUtc=utc;
    }
    public static String time(String utc){
        try{return DateTimeFormatter.ofPattern("MMM d, yyyy · HH:mm:ss z",Locale.getDefault()).withZone(ZoneId.systemDefault()).format(Instant.parse(utc));}
        catch(Exception missing){return "Date unknown";}
    }
}
