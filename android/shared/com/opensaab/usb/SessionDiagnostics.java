// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.File;
import org.json.*;

/** Bounded, fixed-vocabulary lifecycle evidence. No user text or transport payloads. */
public final class SessionDiagnostics {
    public enum Event { START, EXPECTED_STOP, UNEXPECTED_EXIT, PAUSED, RESUMED,
        SECURITY_COLLECTION_REQUESTED, SECURITY_PROCESS_REQUESTED, SECURITY_IMPORTED, RESTART_REQUESTED }
    static final String FILE="diagnostic-events.json";
    static JSONArray sanitize(JSONArray input){
        JSONArray out=new JSONArray();
        for(int i=Math.max(0,input.length()-32);i<input.length();i++)try{
            JSONObject raw=input.getJSONObject(i);
            Event event=Event.valueOf(raw.getString("event"));
            String utc=java.time.Instant.parse(raw.getString("utc")).toString();
            out.put(new JSONObject().put("event",event.name()).put("utc",utc));
        }catch(Exception ignored){}
        return out;
    }
    public static synchronized JSONArray read(File directory){
        if(directory==null)return new JSONArray();
        try{File f=new File(directory,FILE);if(f.length()>16384)return new JSONArray();
            return sanitize(FirmwareStore.json(f).getJSONArray("events"));
        }catch(Exception unavailable){return new JSONArray();}
    }
    public static synchronized void record(File directory,Event event){
        if(directory==null||!directory.isDirectory())return;
        try{JSONArray events=read(directory);events.put(new JSONObject().put("event",event.name()).put("utc",java.time.Instant.now().toString()));
            FirmwareStore.writeJson(new File(directory,FILE),new JSONObject().put("events",sanitize(events)));
        }catch(Exception ignored){} // Diagnostics must not interrupt the session.
    }
    private SessionDiagnostics(){}
}
