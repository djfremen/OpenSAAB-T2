// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.os.Bundle;
import java.io.*;
import java.nio.file.*;
import org.json.JSONObject;

/** Session/API contract tests with synthetic responses; no USB or external requests. */
public final class VehicleSessionInstrumentedTest extends Instrumentation {
    void check(boolean v,String message){if(!v)throw new AssertionError(message);}
    public void onCreate(Bundle args){super.onCreate(args);start();}
    public void onStart(){
        Bundle result=new Bundle();int code=-1;File dir=null;
        try{
            JSONObject probe=new JSONObject().put("vin","YS3FD49YX41000001").put("status","vin_received").put("usb_closed",true).put("request_origin","host-startup-discovery");
            VehicleIdentity v=VehicleSession.fromProbe(probe);check(v.modelYear==2004,"Wrong year");
            probe.put("usb_closed",false);try{VehicleSession.fromProbe(probe);throw new AssertionError("Unclosed probe accepted");}catch(IOException expected){}probe.put("usb_closed",true);
            probe.put("status","vin_no_reply");try{VehicleSession.fromProbe(probe);throw new AssertionError("No reply accepted");}catch(IOException expected){}
            JSONObject payload=new JSONObject().put("success",true).put("vin",v.vin).put("security_code","DO_NOT_SAVE")
                .put("vehicle",new JSONObject().put("model_year",2004).put("platform","9440").put("engine","B207R").put("color",new JSONObject().put("english","Silver Metallic")).put("gearbox","FA57"));
            VehicleIdentity enriched=VehicleSession.applyLookup(v,payload);check(enriched.engine.equals("B207R")&&enriched.color.equals("Silver Metallic"),"Lookup fields missing");
            check(!VehicleSession.json(enriched).toString().contains("DO_NOT_SAVE"),"Security response persisted");
            payload.put("vin","YS3FH46U681000002");try{VehicleSession.applyLookup(v,payload);throw new AssertionError("Changed VIN accepted");}catch(IOException expected){}
            payload.put("vin",v.vin);payload.getJSONObject("vehicle").put("model_year",2008);try{VehicleSession.applyLookup(v,payload);throw new AssertionError("Changed year accepted");}catch(IOException expected){}
            VehicleIdentity offline=VehicleSession.unavailable(v,"unavailable");check(offline.vin.equals(v.vin)&&offline.modelYear==2004,"Offline lost VIN");
            check(VehicleSession.lookup(v,()->true).lookupStatus.equals("cancelled"),"Cancelled lookup attempted network");
            try{VehicleSession.bounded(new ByteArrayInputStream(new byte[11]),10,()->false);throw new AssertionError("Unbounded response");}catch(IOException expected){}
            dir=Files.createTempDirectory(getTargetContext().getCacheDir().toPath(),"vehicle-test-").toFile();File f=new File(dir,VehicleSession.FILE);
            VehicleSession.save(f,enriched);check(VehicleSession.read(f).vin.equals(v.vin),"Session did not survive reload");
            File other=new File(dir,"different-session.json");check(VehicleSession.read(other)==null,"Missing session reused old VIN");
            Files.write(other.toPath(),"broken".getBytes("UTF-8"));check(VehicleSession.read(other)==null,"Corrupt identity accepted");
            DtcReport r=new DtcReport("chipsoft","synthetic",DtcReport.parse("DTC Information\nACC B2429 06 Seat heating\n1 / 1\n"),VehicleSession.read(f));
            check(r.text().contains(v.vin)&&r.text().contains("Silver Metallic")&&new JSONObject(r.json()).getString("vin").equals(v.vin),"VIN/data not in report");
            result.putString("stream","PASS: fresh VIN contract, cleanup required, matching VIN/year lookup, offline VIN, cancellation, bounded responses, private field allowlist, durable session identity, no cross-session reuse, VIN in reports; no USB/network\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(dir!=null){for(File f:dir.listFiles())f.delete();dir.delete();}finish(code,result);}
    }
}
