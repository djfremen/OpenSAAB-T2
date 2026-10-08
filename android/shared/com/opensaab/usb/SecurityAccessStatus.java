// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.time.Instant;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.*;

/** Local processing receipt. API success and a card import never establish vehicle access. */
public final class SecurityAccessStatus {
    /** Advisory age of imported data, not a firmware/ECU authorization expiry. */
    public static final long FRESH_SECONDS=3*60*60;
    public final Properties data=new Properties();
    private static final DateTimeFormatter DISPLAY=DateTimeFormatter.ofPattern("MMM d, HH:mm:ss z",Locale.getDefault()).withZone(ZoneId.systemDefault());
    public SecurityAccessStatus(String vin,String session,Instant now)throws Exception{
        data.setProperty("schema","1");data.setProperty("vehicle_hash",hash(vin.getBytes(StandardCharsets.US_ASCII)));
        data.setProperty("source_session",session);data.setProperty("started_utc",now.toString());
        data.setProperty("stage","processing");
    }
    private SecurityAccessStatus(){}
    public void collected(Instant now){data.setProperty("pre_auth_utc",now.toString());}
    public void processed(String provider,String requestId,Instant now){
        data.setProperty("provider",provider);data.setProperty("processed_utc",now.toString());data.setProperty("stage","processed");
        if(requestId!=null&&requestId.matches("OSSEC-[a-f0-9]{32}"))data.setProperty("request_id",requestId);
    }
    public void imported(byte[] ssa,Instant now)throws Exception{
        if(!data.containsKey("processed_utc"))throw new IllegalStateException("No validated server reply");
        data.setProperty("ssa_hash",hash(ssa));data.setProperty("imported_utc",now.toString());data.setProperty("stage","imported");
    }
    public void failed(Instant now){data.setProperty("failed_utc",now.toString());data.setProperty("stage","failed");}
    public boolean matches(String vin)throws Exception{return data.getProperty("vehicle_hash","").equals(hash(vin.getBytes(StandardCharsets.US_ASCII)));}
    public boolean imported(){return "imported".equals(data.getProperty("stage"));}
    public boolean cardMatches(File card){
        try(RandomAccessFile f=new RandomAccessFile(card,"r")){
            byte[] bytes=new byte[SsaData.SIZE];f.seek(SsaData.OFFSET);f.readFully(bytes);
            return imported()&&data.getProperty("ssa_hash","").equals(hash(bytes));
        }catch(Exception missing){return false;}
    }
    public boolean sameSession(File session){return session!=null&&session.getName().equals(data.getProperty("source_session"));}
    private String time(String name){String value=data.getProperty(name);return value==null?"not completed":DISPLAY.format(Instant.parse(value));}
    public String freshness(boolean cardMatches,Instant now){
        Long age=ageSeconds(cardMatches,now);
        return age==null?"Age unknown":age<FRESH_SECONDS?"Fresh":"Stale";
    }
    private Long ageSeconds(boolean cardMatches,Instant now){
        if(!imported()||!cardMatches||now==null)return null;
        try{
            long seconds=java.time.Duration.between(Instant.parse(data.getProperty("imported_utc")),now).getSeconds();
            return seconds<0?null:seconds;
        }catch(RuntimeException invalid){return null;}
    }
    public String ageDetails(boolean cardMatches,Instant now){
        Long seconds=ageSeconds(cardMatches,now);
        if(seconds==null)return "Data age: unknown (no matching timestamp, card changed, or device clock changed).";
        long minutes=seconds/60;
        String elapsed=minutes<1?"less than a minute":minutes<60?minutes+" min":minutes/60+" h "+minutes%60+" min";
        return "Data age: "+elapsed+" · "+freshness(cardMatches,now)+
            "\nFresh means written less than 3 hours ago; Stale means 3 hours or older. This is an age reminder, not a confirmed vehicle-access expiry. Follow the firmware if it requests security access again.";
    }
    public String summary(boolean sameSession,boolean connected,boolean cardMatches){
        String stage=data.getProperty("stage","");
        if(imported()&&cardMatches)return "Post-auth written · "+time("imported_utc");
        if(imported())return "Previous security data · "+time("imported_utc")+"\nCard has changed since this import.";
        String prefix=sameSession?"Security processing":"Previous security processing";
        if("failed".equals(stage))return prefix+" stopped · "+time("failed_utc");
        if("processed".equals(stage))return prefix+": response received · "+time("processed_utc")+"\nNot yet loaded into the card.";
        return prefix+" started · "+time("started_utc");
    }
    public String details(boolean connected,boolean cardMatches){
        String id=data.getProperty("request_id","");
        return "Pre-auth collected: "+time(data.containsKey("pre_auth_utc")?"pre_auth_utc":"started_utc")+"\nPost-auth received: "+time("processed_utc")+
            "\nPost-auth written: "+time("imported_utc")+
            (data.containsKey("failed_utc")?"\nStopped: "+time("failed_utc"):"")+
            "\nProvider: "+data.getProperty("provider","not recorded")+(id.isEmpty()?"":"\nRequest: "+id)+
            (imported()&&!cardMatches?"\n\nCard has changed since this import.":"")+
            (imported()&&cardMatches?"\n\nSecurity data is loaded. Continue your task in the firmware.":"")+
            (imported()?"\n\n"+ageDetails(cardMatches,Instant.now()):"");
    }
    public void save(File file)throws Exception{
        File tmp=new File(file.getParentFile(),file.getName()+".tmp");
        try(FileOutputStream out=new FileOutputStream(tmp)){data.store(out,"OpenSAAB local security processing receipt; no keys or VIN");out.getFD().sync();}
        Files.move(tmp.toPath(),file.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
    }
    public static SecurityAccessStatus read(File file){
        try{
            if(!file.isFile()||file.length()>8192)return null;
            SecurityAccessStatus status=new SecurityAccessStatus();
            try(InputStream in=new FileInputStream(file)){status.data.load(in);}
            if(!"1".equals(status.data.getProperty("schema")))return null;
            Instant.parse(status.data.getProperty("started_utc"));
            for(String key:new String[]{"pre_auth_utc","processed_utc","imported_utc","failed_utc"})if(status.data.containsKey(key))Instant.parse(status.data.getProperty(key));
            if(!status.data.getProperty("vehicle_hash","").matches("[a-f0-9]{64}"))return null;
            if(!Arrays.asList("processing","processed","imported","failed").contains(status.data.getProperty("stage")))return null;
            if(status.imported()&&(!status.data.containsKey("processed_utc")||!status.data.containsKey("imported_utc")||!status.data.getProperty("ssa_hash","").matches("[a-f0-9]{64}")))return null;
            return status;
        }catch(Exception invalid){return null;}
    }
    private static String hash(byte[] bytes)throws Exception{
        StringBuilder s=new StringBuilder();for(byte b:MessageDigest.getInstance("SHA-256").digest(bytes))s.append(String.format(Locale.ROOT,"%02x",b&255));return s.toString();
    }
}
