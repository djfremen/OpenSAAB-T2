// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.os.*;
import org.json.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.atomic.AtomicBoolean;

/** Native transport effects only. Shared Rust owns startup, commands and VIN/ECM validation. */
public class VlinkerVehicleConnection {
    // Worker processes have separate address spaces; claim one native owner before launching any.
    private static final java.util.concurrent.atomic.AtomicReference<VlinkerVehicleConnection> OWNER=new java.util.concurrent.atomic.AtomicReference<>();
    public static boolean active(){return OWNER.get()!=null;}
    public interface Listener { void changed(JSONObject snapshot); }
    private final Context context;
    interface LinkFactory { VlinkerTransport create() throws Exception; }
    private final LinkFactory linkFactory;
    private final String transportKind,evidenceKind;
    private final boolean enabled;
    private volatile VlinkerTransport link;
    private final Listener listener;
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final AtomicBoolean cancelled=new AtomicBoolean();
    public volatile boolean busy;
    private volatile java.lang.Process core;
    private OutputStream coreInput;
    private InputStream coreOutput;
    private boolean closed,protocolStarted;
    private int attempts,written;
    private File traceDirectory;
    private BufferedWriter transcript;
    private String currentCommand="";
    private String probeMode="",ownerToken="";
    private JSONObject commandAttempts,commandWrites,workflowManifest,transportIdentity;
    VlinkerVehicleConnection(Context c,Listener l,LinkFactory factory,String kind,boolean enabled){this(c,l,factory,kind,enabled,false);}
    VlinkerVehicleConnection(Context c,Listener l,LinkFactory factory,String kind,boolean enabled,boolean fixture){
        if(fixture&&!SimulatorVehicleConnection.available(c))throw new IllegalArgumentException("Synthetic transport requires a Debug emulator");
        context=c;listener=l;linkFactory=factory;transportKind=kind;evidenceKind=fixture?"synthetic-fixture":kind;this.enabled=enabled;
    }
    private void cleanupProtocol(){
        if(!protocolStarted||closed)return;
        try{
            JSONObject cleanup=protocol(op("cancel"));long limit=SystemClock.elapsedRealtime()+4000;
            while(cleanup.optString("state").equals("send")&&SystemClock.elapsedRealtime()<limit){
                currentCommand=cleanup.getString("command");send(op("write").put("command",currentCommand));
                cleanup=new JSONObject().put("state","waiting");
                while(cleanup.optString("state").equals("waiting")&&SystemClock.elapsedRealtime()<limit){
                    JSONObject packet=packet(limit,false);
                    if(packet.optString("type").equals("write_completed")){String command=packet.optString("command");commandWrites.put(command,commandWrites.optInt(command)+1);}
                    else if(packet.optString("type").equals("rx"))cleanup=protocol(op("rx").put("bytes",packet.getJSONArray("bytes")));
                    if(closed)break;
                }
            }
        }catch(Exception ignored){}
    }
    public static VlinkerVehicleConnection direct(Context c,Listener l,android.bluetooth.BluetoothDevice device,boolean compatibility){
        return new VlinkerVehicleConnection(c,l,()->new VlinkerSppTransport(c,device,compatibility),"android-classic-spp",true);
    }
    public static boolean available(Context c){return new File(c.getApplicationInfo().nativeLibraryDir,"libopensaab_connection.so").canExecute();}
    public void startProbe(String mode){
        if(!mode.equals("ecm_info")&&!mode.equals("ecm_dtc"))return;
        if(busy||(!enabled||!available(context)))return;probeMode=mode;ownerToken="";start();
    }
    public void start(){
        if(busy||(!enabled||!available(context)))return;
        if(!OWNER.compareAndSet(null,this)){publish(blockedSnapshot());return;}
        busy=true;cancelled.set(false);closed=false;protocolStarted=false;ownerToken="";attempts=0;written=0;commandAttempts=new JSONObject();commandWrites=new JSONObject();transportIdentity=new JSONObject();
        new Thread(this::run,"opensaab-vlinker-effects").start();
    }
    private static JSONObject blockedSnapshot(){try{return new JSONObject().put("state","blocked").put("message","Finish the active workflow before connecting");}catch(JSONException e){throw new AssertionError(e);}}
    public void stop(){cancelled.set(true);VlinkerTransport current=link;if(current!=null)current.cancelPendingIO();}
    private JSONObject event(String scope,JSONObject value)throws Exception{
        value.put("scope",scope);
        coreInput.write((value.toString()+"\n").getBytes(StandardCharsets.UTF_8));coreInput.flush();
        ByteArrayOutputStream line=new ByteArrayOutputStream();int b;
        while((b=coreOutput.read())!=-1&&b!=10){if(line.size()>=300000)throw new IOException("Shared workflow response too large");line.write(b);}
        if(b==-1)throw new IOException("Shared workflow stopped");
        return new JSONObject(line.toString("UTF-8"));
    }
    private JSONObject connection(JSONObject value)throws Exception{return event("connection",value);}
    private JSONObject protocol(JSONObject value)throws Exception{return event("vlinker",value);}
    private static JSONObject op(String name)throws JSONException{return new JSONObject().put("op",name);}
    private void publish(JSONObject value){ui.post(()->listener.changed(value));}
    private void record(String type,JSONObject value)throws Exception{
        transcript.write(new JSONObject().put("event",type).put("elapsed_realtime_ms",SystemClock.elapsedRealtime()).put("data",value).toString());transcript.newLine();transcript.flush();
    }
    private void send(JSONObject value)throws Exception{
        if(value.optString("op").equals("write")){String command=value.getString("command");commandAttempts.put(command,commandAttempts.optInt(command)+1);}
        record("tx",value);link.send(value);
    }
    private JSONObject packet(long deadline,boolean honorCancel)throws Exception{
        while(SystemClock.elapsedRealtime()<deadline){
            if(honorCancel&&cancelled.get())throw new IOException("Connection cancelled");
            try{
                JSONObject p=link.receive(200);record("rx",p);
                if(p.optString("type").equals("closed"))closed=p.optBoolean("bluetooth_closed",false);
                return p;
            }catch(SocketTimeoutException timeout){
                if(honorCancel&&protocolStarted){JSONObject tick=protocol(op("tick"));if(tick.optString("state").equals("send"))dispatchSend(tick);if(tick.optString("state").equals("failed"))throw new IOException(tick.optString("error"));}
            }
        }
        throw new IOException("Bluetooth transport timed out");
    }
    private void release()throws Exception{
        if(link==null){closed=true;return;}
        if(closed)return;
        send(op("close"));long deadline=SystemClock.elapsedRealtime()+6000;
        while(!closed){JSONObject p=packet(deadline,false);if(p.optString("type").equals("closed")&&!closed)throw new IOException("Bluetooth release was not confirmed");}
    }
    private void dispatchSend(JSONObject state)throws Exception{
        currentCommand=state.getString("command");if(currentCommand.equals("021A900000000000"))attempts++;
        send(op("write").put("command",currentCommand));
    }
    private JSONObject discover()throws Exception{
        JSONObject claim=op("claim");if(!ownerToken.isEmpty())claim.put("owner",ownerToken);
        JSONObject acquired=protocol(claim);if(!acquired.optString("state").equals("idle"))throw new IOException(acquired.optString("error"));
        if(cancelled.get())throw new IOException("Connection cancelled");
        link=linkFactory.create();if(cancelled.get())throw new IOException("Connection cancelled");
        link.open();if(cancelled.get())throw new IOException("Connection cancelled");
        long deadline=SystemClock.elapsedRealtime()+30000;
        JSONObject ready=packet(deadline,true);transportIdentity=ready;
        boolean macSpp=transportKind.equals("android-emulator-local-m4-bluetooth")&&SimulatorVehicleConnection.available(context)
            &&ready.optString("transport").equals("macos-classic-spp")&&ready.optString("uuid").equals(VlinkerSppTransport.SPP_UUID.toString())&&ready.optString("device_name").equals("Carista EVO");
        boolean profile=transportKind.equals("android-classic-spp")
            ?ready.optString("transport").equals("android-classic-spp")&&ready.optString("uuid").equals(VlinkerSppTransport.SPP_UUID.toString())
            :macSpp||(ready.optString("service").equals("18F0")&&ready.optString("write").equals("2AF1")&&ready.optString("notify").equals("2AF0")&&ready.optString("write_type").equals("withResponse"));
        if(!ready.optString("type").equals("ready")||!profile)throw new IOException("Selected Bluetooth transport unavailable");
        JSONObject state=protocol(op("start"));protocolStarted=true;
        while(true){
            if(cancelled.get())throw new IOException("Connection cancelled");
            String phase=state.optString("state");
            if(phase.equals("failed"))throw new IOException(state.optString("error"));
            if(phase.equals("complete"))return state.getJSONObject("result");
            if(phase.equals("send")){
                dispatchSend(state);state=new JSONObject().put("state","waiting");
            }
            JSONObject p=packet(deadline,true);String type=p.optString("type");
            if(type.equals("write_completed")){
                String completed=p.optString("command");commandWrites.put(completed,commandWrites.optInt(completed)+1);
                if(!currentCommand.equals(p.optString("command")))throw new IOException("Unexpected write completion");
                if(currentCommand.equals("021A900000000000"))written++;
            }else if(type.equals("rx"))state=protocol(op("rx").put("bytes",p.getJSONArray("bytes")));
            else if(type.equals("closed"))throw new IOException(p.optString("error","Bluetooth disconnected"));
            else throw new IOException("Unexpected Bluetooth packet");
        }
    }
    private JSONObject lookup(JSONObject request){
        HttpURLConnection http=null;
        try{
            if(cancelled.get())return op("lookup_unavailable");
            http=(HttpURLConnection)new URL(request.getString("url")).openConnection();
            http.setConnectTimeout(5000);http.setReadTimeout(5000);http.setInstanceFollowRedirects(false);http.setRequestProperty("Accept","application/json");
            int status=http.getResponseCode();if(status!=200)return op("lookup_unavailable");
            ByteArrayOutputStream bytes=new ByteArrayOutputStream();long deadline=SystemClock.elapsedRealtime()+10000;
            try(InputStream stream=http.getInputStream()){
                byte[] buffer=new byte[4096];int n;
                while((n=stream.read(buffer))!=-1){if(cancelled.get()||SystemClock.elapsedRealtime()>deadline||bytes.size()+n>request.getInt("maximum_bytes"))throw new IOException("Vehicle lookup interrupted or too large");bytes.write(buffer,0,n);}
            }
            return op("lookup_result").put("http_status",status).put("body",bytes.toString("UTF-8"));
        }catch(Exception unavailable){try{return op("lookup_unavailable");}catch(Exception impossible){throw new AssertionError(impossible);}}
        finally{if(http!=null)http.disconnect();}
    }
    private void runProbe(){
        JSONObject snapshot=null,result=null;String error="";
        try{
            traceDirectory=new File(context.getFilesDir(),(evidenceKind.equals("android-classic-spp")?"vlinker-hscan-tests/":"simulator-hscan-tests/")+java.util.UUID.randomUUID());if(!traceDirectory.mkdirs())throw new IOException("Cannot save probe evidence");
            transcript=new BufferedWriter(new OutputStreamWriter(new FileOutputStream(new File(traceDirectory,"transcript.jsonl")),StandardCharsets.UTF_8));
            core=new ProcessBuilder(new File(context.getApplicationInfo().nativeLibraryDir,"libopensaab_connection.so").toString(),context.getFilesDir().toString()).redirectError(new File(traceDirectory,"core-stderr.log")).start();
            coreInput=core.getOutputStream();coreOutput=core.getInputStream();workflowManifest=event("manifest",op("read"));
            JSONObject configured=protocol(op("configure").put("mode",probeMode));if(!configured.optString("state").equals("idle"))throw new IOException("Unsupported ECM probe");
            publish(new JSONObject().put("state","reading").put("message","Reading a fresh ECM VIN and "+(probeMode.equals("ecm_info")?"software identifiers":"current/history engine codes")+"…"));
            try{result=discover();}catch(Exception failure){error=failure.getMessage();}
            finally{
                // A cancelled/failed diagnostic operation gets one shared stop attempt.
                // Late traffic or missing prompt cannot publish a completed report.
                if(result==null)cleanupProtocol();
                protocolStarted=false;try{release();}catch(Exception failure){error="Bluetooth release was not confirmed";}
            }
            boolean success=result!=null&&closed&&error.isEmpty()&&!cancelled.get();
            JSONObject report=new JSONObject().put("schema",1).put("shared_workflow",workflowManifest).put("selected_device",transportIdentity).put("mode",probeMode).put("transport",evidenceKind).put("app_version",AppBuildProfile.installedVersion(context))
                .put("status",success?"complete":"failed").put("bluetooth_closed",closed).put("host_vehicle_request_attempts",attempts).put("host_vehicle_requests_written",written).put("automatic_replay",false).put("command_attempts",commandAttempts).put("command_writes",commandWrites).put("error",error);
            if(success)report.put("result",result);
            if(success&&!probeMode.isEmpty()){
                JSONObject saved=new JSONObject(report.toString()).put("op","save").put("observed_utc",java.time.Instant.now().toString()).put("app_build",String.valueOf(context.getPackageManager().getPackageInfo(context.getPackageName(),0).versionCode));
                try{JSONObject receipt=event("reports",saved);if(receipt.has("error"))report.put("report_save_error",receipt.getString("error"));else report.put("saved_report_id",receipt.getString("id"));}
                catch(Exception storage){report.put("report_save_error","Diagnostic report could not be saved");}
            }
            Files.write(new File(traceDirectory,"result.json").toPath(),report.toString(2).getBytes(StandardCharsets.UTF_8));
            snapshot=new JSONObject().put("state",success?"complete":"failed").put("message",success?result.optString("display"):"ECM probe failed · "+error).put("report",report);
        }catch(Exception failure){try{snapshot=new JSONObject().put("state","failed").put("message","ECM probe unavailable · "+failure.getMessage());}catch(Exception impossible){throw new AssertionError(impossible);}}
        finally{
            if(link!=null){link.shutdown();link=null;}if(core!=null){core.destroy();core=null;}try{if(transcript!=null)transcript.close();}catch(Exception ignored){}
            final JSONObject terminal=snapshot;ui.post(()->{OWNER.compareAndSet(this,null);busy=false;probeMode="";if(terminal!=null)listener.changed(terminal);});
        }
    }
    private void run(){
        if(!probeMode.isEmpty()){runProbe();return;}
        JSONObject result=null,snapshot=null;String error="";boolean success=false;
        try{
            traceDirectory=new File(context.getFilesDir(),(evidenceKind.equals("android-classic-spp")?"vlinker-startup-tests/":"simulator-vlinker-tests/")+java.util.UUID.randomUUID());if(!traceDirectory.mkdirs())throw new IOException("Cannot save connection evidence");
            transcript=new BufferedWriter(new OutputStreamWriter(new FileOutputStream(new File(traceDirectory,"transcript.jsonl")),StandardCharsets.UTF_8));
            core=new ProcessBuilder(new File(context.getApplicationInfo().nativeLibraryDir,"libopensaab_connection.so").toString(),context.getFilesDir().toString()).redirectError(new File(traceDirectory,"core-stderr.log")).start();
            coreInput=core.getOutputStream();coreOutput=core.getInputStream();workflowManifest=event("manifest",op("read"));
            snapshot=connection(op("begin").put("firmware_ready",true));
            if(!snapshot.optString("state").equals("selecting"))throw new IOException(snapshot.optString("message"));
            ownerToken=snapshot.optString("attempt_id");
            snapshot=connection(op("select_adapter").put("adapter","bluetooth_mic").put("transport_available",true));publish(snapshot);
            try{result=discover();}catch(Exception failure){error=failure.getMessage();}
            finally{if(result==null)cleanupProtocol();protocolStarted=false;try{release();}catch(Exception failure){error="Bluetooth release was not confirmed";}}
            success=result!=null&&closed&&error.isEmpty()&&!cancelled.get();
            JSONObject report=new JSONObject().put("schema",1).put("shared_workflow",workflowManifest).put("selected_device",transportIdentity).put("transport",evidenceKind).put("app_version",AppBuildProfile.installedVersion(context))
                .put("status",success?"vin_received":"failed").put("bluetooth_closed",closed).put("host_vehicle_request_attempts",attempts).put("host_vehicle_requests_written",written).put("automatic_replay",false).put("command_attempts",commandAttempts).put("command_writes",commandWrites).put("error",error);
            if(success)report.put("result",result);
            Files.write(new File(traceDirectory,"result.json").toPath(),report.toString(2).getBytes(StandardCharsets.UTF_8));
            if(cancelled.get()){
                snapshot=connection(op("cancel"));snapshot=connection(op("transport_closed").put("bluetooth_closed",closed));
            }else{
                snapshot=connection(op("probe_result").put("report",report));publish(snapshot);
                if(snapshot.optString("effect").equals("lookup"))snapshot=connection(cancelled.get()?op("cancel"):lookup(snapshot));
                if(cancelled.get())snapshot=connection(op("cancel"));
            }
        }catch(Exception failure){
            try{if(core!=null){snapshot=connection(op("cancel"));if(snapshot.optString("state").equals("stopping"))snapshot=connection(op("transport_closed").put("bluetooth_closed",closed));}}catch(Exception ignored){}
            if(snapshot==null)try{snapshot=new JSONObject().put("state","failed").put("busy",false).put("message","Connection unavailable · "+failure.getMessage());}catch(Exception impossible){throw new AssertionError(impossible);}
        }finally{
            if(link!=null){link.shutdown();link=null;}
            if(core!=null){core.destroy();core=null;}try{if(transcript!=null)transcript.close();}catch(Exception ignored){}
            final JSONObject terminal=snapshot;
            ui.post(()->{OWNER.compareAndSet(this,null);busy=false;if(terminal!=null)listener.changed(terminal);});
        }
    }
}
