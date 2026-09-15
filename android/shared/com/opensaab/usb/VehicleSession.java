// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** VIN-first session storage and bounded OpenSAAB vehicle lookup. No security values are persisted. */
public final class VehicleSession {
    public static final String FILE="vehicle-session.json";
    private static final String ENDPOINT="https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/lookup/";
    static VehicleIdentity fromProbe(JSONObject p)throws Exception{
        if(!p.optBoolean("usb_closed") || !"vin_received".equals(p.optString("status"))
            || !"host-startup-discovery".equals(p.optString("request_origin")))throw new IOException("No verified VIN reply");
        String vin=p.getString("vin");
        return new VehicleIdentity(vin,java.time.Instant.now().toString(),"Chipsoft P-bus ECU VIN reply",VehicleIdentity.year(vin),"pending","","","","","");
    }
    static String field(JSONObject o,String key){
        Object v=o.opt(key);if(!(v instanceof String))return "";
        String text=((String)v).replaceAll("[\\p{Cntrl}]"," ").trim();return text.substring(0,Math.min(160,text.length()));
    }
    static VehicleIdentity applyLookup(VehicleIdentity v,JSONObject p)throws Exception{
        if(!v.vin.equals(p.optString("vin")))throw new IOException("Vehicle lookup returned a different VIN");
        if(!p.optBoolean("success"))return unavailable(v,"not_found");
        JSONObject car=p.getJSONObject("vehicle"), paint=car.optJSONObject("color");
        int year=car.optInt("model_year",v.modelYear);
        if(v.modelYear>0 && year!=v.modelYear)throw new IOException("Vehicle lookup returned a different model year");
        return new VehicleIdentity(v.vin,v.observedUtc,v.source,year,"available",field(car,"platform"),field(car,"engine"),paint==null?"":field(paint,"english"),field(car,"gearbox"),field(car,"body_display"));
    }
    static VehicleIdentity unavailable(VehicleIdentity v,String reason){return new VehicleIdentity(v.vin,v.observedUtc,v.source,v.modelYear,reason,"","","","","");}
    static VehicleIdentity lookup(VehicleIdentity vehicle,BooleanSupplier cancelled){
        HttpURLConnection http=null;
        try{
            if(cancelled.getAsBoolean())return unavailable(vehicle,"cancelled");
            http=(HttpURLConnection)new URL(ENDPOINT+vehicle.vin+"?include_rpo=true&include_options=false&include_recalls=false").openConnection();
            http.setConnectTimeout(5000);http.setReadTimeout(7000);http.setInstanceFollowRedirects(false);
            http.setRequestProperty("Accept","application/json");
            if(http.getResponseCode()!=200)throw new IOException("Lookup unavailable");
            byte[] bytes;try(InputStream in=http.getInputStream()){bytes=bounded(in,256*1024,cancelled);}
            if(cancelled.getAsBoolean())return unavailable(vehicle,"cancelled");
            return applyLookup(vehicle,new JSONObject(new String(bytes,StandardCharsets.UTF_8)));
        }catch(Exception unavailable){return unavailable(vehicle,cancelled.getAsBoolean()?"cancelled":"unavailable");}
        finally{if(http!=null)http.disconnect();}
    }
    static byte[] bounded(InputStream in,int max,BooleanSupplier cancelled)throws IOException{
        ByteArrayOutputStream out=new ByteArrayOutputStream();byte[] buffer=new byte[4096];int n;
        long deadline=System.nanoTime()+java.util.concurrent.TimeUnit.SECONDS.toNanos(10);
        while((n=in.read(buffer))!=-1){if(cancelled.getAsBoolean()||System.nanoTime()>deadline||out.size()+n>max)throw new IOException("Vehicle data interrupted or oversized");out.write(buffer,0,n);}return out.toByteArray();
    }
    static JSONObject json(VehicleIdentity v)throws Exception{
        return new JSONObject().put("schema",1).put("vin",v.vin).put("observed_utc",v.observedUtc).put("source",v.source).put("model_year",v.modelYear)
            .put("lookup_status",v.lookupStatus).put("platform",v.platform).put("engine",v.engine).put("color",v.color).put("gearbox",v.gearbox).put("body",v.body);
    }
    public static VehicleIdentity read(File file){
        try{
            if(!file.isFile()||file.length()>8192)return null;
            JSONObject p=new JSONObject(new String(Files.readAllBytes(file.toPath()),StandardCharsets.UTF_8));
            return new VehicleIdentity(p.getString("vin"),p.getString("observed_utc"),p.getString("source"),p.getInt("model_year"),p.getString("lookup_status"),field(p,"platform"),field(p,"engine"),field(p,"color"),field(p,"gearbox"),field(p,"body"));
        }catch(Exception unavailable){return null;}
    }
    static void save(File file,VehicleIdentity v)throws Exception{
        File tmp=new File(file.getParentFile(),file.getName()+".tmp");
        Files.write(tmp.toPath(),json(v).toString(2).getBytes(StandardCharsets.UTF_8));
        Files.move(tmp.toPath(),file.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
    }
}
