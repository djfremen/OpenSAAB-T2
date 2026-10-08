// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.os.SystemClock;
import java.io.*;
import java.security.MessageDigest;
import java.util.*;
import org.json.*;

/** OS facts for offline sessions. No key values, screen text, or mutable SSA bytes. */
public final class OfflineSessionEvidence {
    public static final long INSTRUCTION_LIMIT=50000000000L, DEADLINE_MS=1800000L;
    private File directory;
    private long started,inputs,frames,lastInput=-1,lastFrame=-1;
    private String stopIntent="none";
    public static String id(File dir){
        if(dir==null)return "";String name=dir.getName();
        if(name.matches("(?:chipsoft|native|offline)-[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}"))return name;
        if(name.matches("[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}"))return "offline-"+name;
        return "";
    }
    public static String digest(File file)throws Exception {
        MessageDigest hash=MessageDigest.getInstance("SHA-256");
        try(InputStream in=new FileInputStream(file)){byte[] b=new byte[65536];int n;while((n=in.read(b))!=-1)hash.update(b,0,n);}
        StringBuilder out=new StringBuilder();for(byte b:hash.digest())out.append(String.format(Locale.ROOT,"%02x",b&255));return out.toString();
    }
    public void begin(File run,File executable,File firmware){
        synchronized(this){directory=run;started=SystemClock.elapsedRealtime();inputs=frames=0;lastInput=lastFrame=-1;stopIntent="none";}
        try{
            JSONObject hashes=new JSONObject();
            for(String name:new String[]{"eprom.bin","opsys.dwn","candi.bin","card.bin"})hashes.put(name,digest(new File(firmware,name)));
            FirmwareStore.writeJson(new File(run,"emulation-context.json"),new JSONObject().put("schema",1)
                .put("session_id",id(run)).put("profile","offline_research").put("started_utc",java.time.Instant.now().toString())
                .put("engine_sha256",digest(executable)).put("firmware_sha256",hashes)
                .put("instruction_limit",INSTRUCTION_LIMIT).put("deadline_ms",DEADLINE_MS));
        }catch(Exception unavailable){
            try{FirmwareStore.writeJson(new File(run,"emulation-context.json"),new JSONObject().put("schema",1).put("session_id",id(run)).put("unavailable",true));}catch(Exception ignored){}
        }
    }
    public synchronized void input(){if(directory!=null){inputs++;lastInput=SystemClock.elapsedRealtime()-started;}}
    public synchronized void frame(){if(directory!=null){frames++;lastFrame=SystemClock.elapsedRealtime()-started;}}
    public synchronized void requestStop(boolean background){if(directory!=null&&stopIntent.equals("none"))stopIntent=background?"background_stop":"user_stop";}
    public synchronized void ended(Integer exitCode,boolean deadline,boolean forced,boolean launchFailed){
        if(directory==null)return;
        try{
            String reason=!stopIntent.equals("none")?stopIntent:launchFailed?"startup_failure":deadline?"host_deadline":forced?"forced_stop":"process_exit";
            JSONObject end=new JSONObject().put("reason",reason).put("elapsed_ms",SystemClock.elapsedRealtime()-started)
                .put("input_count",inputs).put("frame_count",frames).put("forced_stop",forced);
            if(lastInput>=0)end.put("last_input_elapsed_ms",lastInput);if(lastFrame>=0)end.put("last_frame_elapsed_ms",lastFrame);
            if(exitCode!=null)end.put("exit_code",exitCode);
            FirmwareStore.writeJson(new File(directory,"session-end.json"),end);
        }catch(Exception ignored){}finally{directory=null;}
    }
    private static void numbers(JSONObject raw,JSONObject safe,String...keys)throws JSONException{
        for(String key:keys){Object v=raw.opt(key);if((v instanceof Integer||v instanceof Long)&&((Number)v).longValue()>=0)safe.put(key,v);}
    }
    private static void label(JSONObject raw,JSONObject safe,String key,String...values)throws JSONException{
        String value=raw.optString(key);if(Arrays.asList(values).contains(value))safe.put(key,value);
    }
    static JSONObject nativeEvidence(JSONObject raw)throws JSONException{
        if(raw.optInt("schema")!=1)return new JSONObject().put("unavailable",true);
        JSONObject safe=new JSONObject().put("schema",1);
        label(raw,safe,"reason","output_failure","adapter_failure","candi_stopped","instruction_budget","operator_or_deadline","operator_stop","guest_bootstrap_failure","guest_cpu_fault","link_unavailable","completed","unknown");
        label(raw,safe,"stage","link_unavailable","firmware_missing","vehicle_link_wait","main_menu","model_year","dtc_menu","diagnostics_menu","other_unknown");
        if(!safe.has("reason"))safe.put("reason","unknown");if(!safe.has("stage"))safe.put("stage","other_unknown");
        JSONObject candi=raw.optJSONObject("candi");if(candi!=null){JSONObject c=new JSONObject();
            label(candi,c,"reason","unsupported_access","cpu_fault","instruction_budget","host_cancelled","none","unknown");
            numbers(candi,c,"attempted","completed","cycles","pc","serial_rx_bytes","serial_rx_breaks","serial_tx_bytes","adapter_tx_confirmations","address","width");
            label(candi,c,"access","read","write");
            for(String key:new String[]{"native_uart","external_tx"})if(candi.opt(key) instanceof Boolean)c.put(key,candi.getBoolean(key));safe.put("candi",c);
        }return safe;
    }
    static JSONObject context(JSONObject raw)throws JSONException{
        JSONObject safe=new JSONObject();if(raw.optInt("schema")!=1||(!raw.optBoolean("unavailable")&&!raw.optString("profile").equals("offline_research")))return safe.put("unavailable",true);
        safe.put("schema",1);String id=raw.optString("session_id");if(id.matches("offline-[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}"))safe.put("session_id",id);
        label(raw,safe,"profile","offline_research");numbers(raw,safe,"instruction_limit","deadline_ms");
        String hash=raw.optString("engine_sha256");if(hash.matches("[a-f0-9]{64}"))safe.put("engine_sha256",hash);
        try{safe.put("started_utc",java.time.Instant.parse(raw.getString("started_utc")).toString());}catch(Exception ignored){}
        JSONObject hashes=raw.optJSONObject("firmware_sha256"),h=new JSONObject();if(hashes!=null)for(String name:new String[]{"eprom.bin","opsys.dwn","candi.bin","card.bin"}){
            hash=hashes.optString(name);if(hash.matches("[a-f0-9]{64}"))h.put(name,hash);
        }if(h.length()>0)safe.put("firmware_sha256",h);if(raw.optBoolean("unavailable"))safe.put("unavailable",true);return safe;
    }
    static JSONObject end(JSONObject raw)throws JSONException{
        JSONObject safe=new JSONObject();label(raw,safe,"reason","background_stop","user_stop","startup_failure","host_deadline","forced_stop","process_exit");
        numbers(raw,safe,"elapsed_ms","input_count","frame_count","last_input_elapsed_ms","last_frame_elapsed_ms");
        Object code=raw.opt("exit_code");if(code instanceof Integer)safe.put("exit_code",code);
        if(raw.opt("forced_stop") instanceof Boolean)safe.put("forced_stop",raw.getBoolean("forced_stop"));return safe;
    }
    static void collect(File dir,JSONObject session){
        if(!id(dir).startsWith("offline-"))return;
        try{session.put("emulation_context",new JSONObject().put("unavailable",true));session.put("session_end",new JSONObject().put("unavailable",true));session.put("guest_crash",new File(dir,"crash.json").isFile());}catch(JSONException ignored){}
        for(String name:new String[]{"emulation-context.json","session-end.json"})try{
            File f=new File(dir,name);if(!f.isFile()||f.length()>4096||!f.getCanonicalFile().getParentFile().equals(dir.getCanonicalFile()))continue;
            session.put(name.equals("session-end.json")?"session_end":"emulation_context",name.equals("session-end.json")?end(FirmwareStore.json(f)):context(FirmwareStore.json(f)));
        }catch(Exception unavailable){}
    }
}
