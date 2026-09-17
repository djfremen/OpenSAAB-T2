// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;
import org.json.*;

/** Bounded structural diagnostics: never export raw bus payloads, VINs or SSA files. */
public final class SupportReports {
    private static final String[] EVENTS={"USB_OPEN","USB_CLOSED","USB_TX","USB_RX","CAN_TX","CAN_RX","TIMEOUT","DISCONNECT","PANIC","ERROR","FAILED","NEGATIVE RESPONSE"};
    public static JSONObject logSummary(File file)throws Exception{
        JSONObject out=new JSONObject().put("source",file.getName()).put("file_bytes",file.length());Map<String,Integer> counts=new LinkedHashMap<>();
        long start=Math.max(0,file.length()-262144);out.put("tail_only",start>0);
        try(RandomAccessFile in=new RandomAccessFile(file,"r")){
            in.seek(start);byte[] b=new byte[(int)Math.min(262144,in.length()-start)];in.readFully(b);String text=new String(b,StandardCharsets.UTF_8);if(start>0){int nl=text.indexOf('\n');text=nl<0?"":text.substring(nl+1);}
            for(String line:text.split("\\n")){String upper=line.toUpperCase(Locale.ROOT);for(String event:EVENTS)if(upper.contains(event))counts.put(event,counts.getOrDefault(event,0)+1);}
        }
        out.put("event_counts",new JSONObject(counts));return out;
    }
    public static void recordError(Context context,Throwable error,boolean crash){
        try{
            JSONObject j=new JSONObject().put("utc",java.time.Instant.now().toString()).put("kind",crash?"uncaught_java_exception":"handled_error").put("exception_class",error.getClass().getName());
            JSONArray stack=new JSONArray();StackTraceElement[] frames=error.getStackTrace();for(int i=0;i<Math.min(frames.length,24);i++)stack.put(frames[i].getClassName()+"."+frames[i].getMethodName()+":"+frames[i].getLineNumber());j.put("stack",stack);
            FirmwareStore.writeJson(new File(context.getFilesDir(),crash?"last-app-crash.json":"last-app-error.json"),j);
        }catch(Throwable ignored){} // Error reporting must never replace the original error handler.
    }
    static List<File> sessions(File files){
        List<File> out=new ArrayList<>();File[] children=files.listFiles();
        if(children!=null)for(File f:children)if(f.isDirectory()&&f.getName().matches("(chipsoft|native)-[a-f0-9-]+"))out.add(f);
        File[] offline=new File(files,"sessions").listFiles();if(offline!=null)for(File f:offline)if(f.isDirectory())out.add(f);
        out.sort((a,b)->Long.compare(b.lastModified(),a.lastModified()));return out.subList(0,Math.min(5,out.size()));
    }
    public static JSONObject collect(Context c,String description)throws Exception{
        JSONObject report=new JSONObject().put("format",1).put("created_utc",java.time.Instant.now().toString()).put("description",description.length()>2000?description.substring(0,2000):description);
        android.content.pm.PackageInfo p=c.getPackageManager().getPackageInfo(c.getPackageName(),0);
        report.put("build_profile",AppBuildProfile.name(c)).put("apk_abi",AppBuildProfile.abi(c));
        report.put("app",c.getPackageName()).put("version",p.versionName==null?"development":p.versionName).put("version_code",p.versionCode).put("android_api",android.os.Build.VERSION.SDK_INT).put("device_model",android.os.Build.MODEL).put("abis",new JSONArray(Arrays.asList(android.os.Build.SUPPORTED_ABIS)));
        report.put("device_resources",PerformanceReport.device(c));
        File health=new File(c.getFilesDir(),"last-emulator-health.json");
        if(health.isFile()&&health.length()<4096)try{
            JSONObject raw=FirmwareStore.json(health),safe=new JSONObject();
            String reason=raw.optString("reason");
            if(Arrays.asList("ui_unresponsive","emulator_heartbeat_missing","input_without_display_response","unexpected_emulator_exit").contains(reason)){
                safe.put("reason",reason).put("utc",java.time.Instant.parse(raw.getString("utc")).toString());
                for(String key:new String[]{"elapsed_ms","heartbeat_age_ms","input_wait_ms","ui_delay_ms"})if(raw.opt(key) instanceof Number)safe.put(key,raw.getLong(key));
                report.put("emulator_health",safe);
            }
        }catch(Exception ignored){}
        report.put("privacy","Raw logs, CAN payloads, VIN, SSA, security responses, credentials, firmware and screenshots are excluded. Description is user-provided. Event counts cover bounded log tails only.");
        JSONArray runs=new JSONArray();File files=c.getFilesDir();
        for(File dir:sessions(files)){
            JSONObject session=new JSONObject().put("adapter",dir.getName().startsWith("chipsoft-")?"chipsoft":dir.getName().startsWith("native-")?"nano":"offline").put("modified_utc",java.time.Instant.ofEpochMilli(dir.lastModified()).toString());JSONArray logs=new JSONArray();
            for(String name:new String[]{"usb.log","native-process.log","rust.log","tech2.log","console.log"}){File f=new File(dir,name);if(f.isFile()&&f.getCanonicalFile().getParentFile().equals(dir.getCanonicalFile()))try{logs.put(logSummary(f));}catch(IOException e){logs.put(new JSONObject().put("source",name).put("unavailable",true));}}
            File outcome=new File(dir,"report.json");
            if(outcome.isFile()&&outcome.length()<1048576)try{
                JSONObject raw=FirmwareStore.json(outcome),safe=new JSONObject();
                for(String key:new String[]{"exit_code","instructions","pc","host_cancelled_operations"})
                    if(raw.opt(key) instanceof Number)safe.put(key,raw.opt(key));
                String status=raw.optString("status");
                if(Arrays.asList("complete","incomplete","error","failed","success","cancelled").contains(status))safe.put("status",status);
                session.put("emulator_outcome",safe);
            }catch(Exception unavailable){}
            session.put("native_performance",PerformanceReport.session(dir));
            session.put("logs",logs);runs.put(session);
        }
        report.put("recent_sessions",runs);
        report.put("connection_attempts",ConnectionAttempt.collect(c));
        SecurityAccessStatus security=SecurityAccessStatus.read(new File(c.getNoBackupFilesDir(),"security-processing-status.properties"));
        if(security!=null)try{
            JSONObject safe=new JSONObject().put("stage",security.data.getProperty("stage")).put("vehicle_access_verified",false);
            for(String key:new String[]{"started_utc","processed_utc","imported_utc","failed_utc"})if(security.data.containsKey(key))safe.put(key,java.time.Instant.parse(security.data.getProperty(key)).toString());
            String provider=security.data.getProperty("provider","");if(provider.equals("OpenSAAB")||provider.equals("Bojer"))safe.put("provider",provider);
            String request=security.data.getProperty("request_id","");if(request.matches("OSSEC-[a-f0-9]{32}"))safe.put("request_id",request);
            report.put("security_processing",safe);
        }catch(Exception ignored){}
        File reset=new File(c.getNoBackupFilesDir(),"security-reset.json");
        if(reset.isFile()&&reset.length()<8192)try{
            String utc=java.time.Instant.parse(FirmwareStore.json(reset).getString("cleared_utc")).toString();
            report.put("security_reset",new JSONObject().put("cleared_utc",utc));
        }catch(Exception ignored){}

        if(android.os.Build.VERSION.SDK_INT>=30){
            JSONArray exits=new JSONArray();
            try{
                android.app.ActivityManager manager=(android.app.ActivityManager)c.getSystemService(Context.ACTIVITY_SERVICE);
                for(android.app.ApplicationExitInfo exit:manager.getHistoricalProcessExitReasons(c.getPackageName(),0,5))
                    exits.put(new JSONObject().put("timestamp_ms",exit.getTimestamp()).put("reason",exit.getReason()).put("status",exit.getStatus()).put("pss_kib",exit.getPss()).put("rss_kib",exit.getRss()));
            }catch(RuntimeException unavailable){}
            report.put("android_process_exits",exits);
        }
        for(String name:new String[]{"last-app-crash.json","last-app-error.json"}){File f=new File(files,name);if(f.isFile())try{report.put(name,FirmwareStore.json(f));}catch(Exception ignored){}}
        return report;
    }
    public static File save(Context c,JSONObject report)throws Exception{
        File root=new File(c.getFilesDir(),"support-reports");Files.createDirectories(root.toPath());
        File file=new File(root,"android_support_"+UUID.randomUUID()+".zip"),tmp=File.createTempFile("report-",".tmp",root);
        try{try(ZipOutputStream zip=new ZipOutputStream(new FileOutputStream(tmp))){zip.putNextEntry(new ZipEntry("diagnostics.json"));zip.write(report.toString(2).getBytes(StandardCharsets.UTF_8));zip.closeEntry();}Files.move(tmp.toPath(),file.toPath(),StandardCopyOption.ATOMIC_MOVE);}finally{tmp.delete();}
        File[] old=root.listFiles((d,n)->n.matches("android_support_[a-f0-9-]+\\.zip"));
        if(old!=null){Arrays.sort(old,Comparator.comparingLong(File::lastModified));for(int i=0;i<old.length-8;i++)old[i].delete();}
        return file;
    }
    private SupportReports(){}
}
