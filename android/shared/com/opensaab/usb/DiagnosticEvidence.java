// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.util.*;
import org.json.*;
/** Evidence schema 2: fixed labels, generated IDs and bounded numbers only. */
public final class DiagnosticEvidence {
  static final String[] FILES={"report.json","crash.json","session-end.json","host-performance.json","connection-evidence.json","cleanup-evidence.json","result.json","host-release.json"};
  static String state(File dir,String name){
    try {File f=new File(dir,name);if(!f.exists())return "missing";if(!f.isFile()||!f.getCanonicalFile().getParentFile().equals(dir.getCanonicalFile()))return "unreadable";if(f.length()>1048576)return "oversize";FirmwareStore.json(f);return "available";}
    catch(Exception e){return "invalid";}
  }
  static JSONObject read(File dir,String name){try{if(state(dir,name).equals("available"))return FirmwareStore.json(new File(dir,name));}catch(Exception ignored){}return new JSONObject();}
  static void label(JSONObject r,JSONObject o,String k,String... allowed)throws JSONException {String s=r.optString(k);if(Arrays.asList(allowed).contains(s))o.put(k,s);}
  static void integer(JSONObject r,JSONObject o,String k)throws JSONException {Object v=r.opt(k);if((v instanceof Integer||v instanceof Long)&&((Number)v).longValue()>=Integer.MIN_VALUE&&((Number)v).longValue()<=Integer.MAX_VALUE)o.put(k,v);}
  static String adapter(File dir,String fallback){return read(dir,"host-release.json").optString("adapter").equals("mxplus")?"obdlink":fallback;}
  public static JSONObject collect(File dir,String adapter)throws JSONException {
    adapter=adapter(dir,adapter);
    JSONObject out=new JSONObject().put("schema",2).put("adapter",Arrays.asList("chipsoft","nano","mdi","mongoose","obdlink","vlinker","offline").contains(adapter)?adapter:"unknown");
    out.put("session_mode",adapter.equals("offline")?"offline":"unknown").put("last_operation","unknown");
    JSONObject artifacts=new JSONObject();for(String n:FILES)artifacts.put(n,state(dir,n));out.put("artifacts",artifacts);
    JSONObject binding=read(dir,"connection-evidence.json");
    String id=binding.optString("attempt_id");try{if(UUID.fromString(id).toString().equals(id))out.put("attempt_id",id);}catch(Exception ignored){}
    label(binding,out,"session_mode","offline","read_only","full_native","security_seeds","unknown");
    label(binding,out,"last_operation","connection","vin","dtc_read","dtc_clear","ecu_info","engine_data","security","original_menus","manual_input","unknown");
    label(binding,out,"primary_reason","NONE","NO_ADAPTER","MULTIPLE_ADAPTERS","PERMISSION_DENIED","DISCONNECTED","TIMEOUT","IO_ERROR","PROTOCOL_OR_PROCESS_ERROR","VIN_UNAVAILABLE","CLEANUP_FAILED","USER_STOP","SETUP_REQUIRED");
    JSONObject cleanup=binding.optJSONObject("cleanup");if(cleanup!=null){JSONObject safe=new JSONObject();integer(cleanup,safe,"child_exit_code");
      for(String k:new String[]{"usb_release","protocol_cleanup"})label(cleanup,safe,k,"succeeded","failed","unknown","not_attempted");
      if(cleanup.opt("forced_termination") instanceof Boolean)safe.put("forced_termination",cleanup.getBoolean("forced_termination"));out.put("cleanup",safe);}
    JSONObject host=read(dir,"host-release.json");
    if(host.optString("adapter").equals("mxplus")){
      JSONObject safe=new JSONObject();
      label(host,safe,"bluetooth_release","succeeded","failed","unknown","retained_for_security");
      label(host,safe,"protocol_cleanup","succeeded","failed","unknown");integer(host,safe,"child_exit_code");
      for(String key:new String[]{"process_dead","forced_termination","security_transport_retained"})if(host.opt(key) instanceof Boolean)safe.put(key,host.getBoolean(key));
      safe.put("workflow_qualified",false);
      JSONObject timing=host.optJSONObject("transport_evidence");if(timing!=null){JSONObject times=new JSONObject();
        for(String key:new String[]{"connect_started_elapsed_ms","prior_confirmed_release_elapsed_ms","release_to_connect_ms","first_probe_write_elapsed_ms","first_vehicle_write_elapsed_ms","confirmed_release_elapsed_ms"}){
          Object value=timing.opt(key);if(value==JSONObject.NULL)times.put(key,JSONObject.NULL);
          else if((value instanceof Integer||value instanceof Long)&&((Number)value).longValue()>=0)times.put(key,value);
        }
        label(timing,times,"clock","android_elapsed_realtime");safe.put("timing",times);
      }
      out.put("host_transport",safe);
    }
    JSONObject crash=read(dir,"crash.json");if(crash.length()>0){JSONObject fault=new JSONObject();long pc=crash.optLong("pc",-1);if(pc>=0&&pc<=4294967295L)fault.put("pc",pc);
      JSONArray pcs=crash.optJSONArray("recent_pcs"),safe=new JSONArray();if(pcs!=null)for(int i=Math.max(0,pcs.length()-32);i<pcs.length();i++){Object v=pcs.opt(i);if((v instanceof Integer||v instanceof Long)&&((Number)v).longValue()>=0&&((Number)v).longValue()<=4294967295L)safe.put(v);}fault.put("recent_pcs",safe);out.put("terminal_fault",fault);}
    JSONObject result=read(dir,"result.json"),startup=new JSONObject();
    for(String k:new String[]{"startup_complete","query_record_verified","installed_record_verified","channel_cleanup","usb_closed"})if(result.opt(k) instanceof Boolean)startup.put(k,result.getBoolean(k));
    for(String k:new String[]{"fresh_dh_exchanges","vehicle_tx"}){Object v=result.opt(k);if((v instanceof Integer||v instanceof Long)&&((Number)v).longValue()>=0)startup.put(k,v);}
    if(startup.length()>0)out.put("startup_result",startup);
    return out;
  }
}
