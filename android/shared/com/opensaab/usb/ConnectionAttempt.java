// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.os.SystemClock;
import java.io.File;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Small, private connection records. No USB payloads, VIN, serial, key or exception message. */
public final class ConnectionAttempt {
    public enum Adapter { SELECTION, CHIPSOFT, NANO }
    public enum Stage { DETECTION, PERMISSION, USB_OPEN, INTERFACE_CLAIM, TRANSPORT_START,
        IDENTIFICATION, CHANNEL_OPEN, VIN_REQUEST, FIRMWARE_START, SESSION, CLEANUP }
    public enum Outcome { IN_PROGRESS, COMPLETED, FAILED, CANCELLED }
    public enum Reason { NONE, NO_ADAPTER, MULTIPLE_ADAPTERS, PERMISSION_DENIED, DISCONNECTED,
        TIMEOUT, IO_ERROR, PROTOCOL_OR_PROCESS_ERROR, VIN_UNAVAILABLE, CLEANUP_FAILED, USER_STOP }
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(r -> new Thread(r,"connection-reports"));
    private final File root;
    private final String id=UUID.randomUUID().toString(), created=java.time.Instant.now().toString();
    private final long began=SystemClock.elapsedRealtime();
    private final Adapter adapter;
    private final JSONArray stages=new JSONArray();
    private Stage stage;
    private Outcome outcome=Outcome.IN_PROGRESS;
    private Reason reason=Reason.NONE;
    private Integer vendor,product;

    public ConnectionAttempt(Context c, Adapter adapter) {
        this.root=new File(c.getFilesDir(),"connection-attempts");this.adapter=adapter;
        stage(Stage.DETECTION);
    }
    public synchronized void device(int vendor,int product) { this.vendor=vendor;this.product=product;save(); }
    public synchronized void stage(Stage next) {
        if(outcome!=Outcome.IN_PROGRESS || next==stage)return;
        stage=next;
        try { if(stages.length()<24)stages.put(new JSONObject().put("stage",next.name()).put("elapsed_ms",elapsed())); } catch(JSONException ignored) {}
        save();
    }
    public synchronized void finish(Outcome result,Reason why) {
        if(outcome!=Outcome.IN_PROGRESS)return;
        outcome=result;reason=why;save();
    }
    public void failure(Throwable e) {
        finish(Outcome.FAILED,e instanceof java.net.SocketTimeoutException || e instanceof TimeoutException
            ?Reason.TIMEOUT:e instanceof java.io.IOException?Reason.IO_ERROR:Reason.PROTOCOL_OR_PROCESS_ERROR);
    }
    public synchronized boolean failed(){return outcome==Outcome.FAILED;}
    private long elapsed(){return Math.max(0,SystemClock.elapsedRealtime()-began);}
    private synchronized void save() {
        try {
            JSONObject j=new JSONObject().put("format",1).put("attempt_id",id).put("created_utc",created)
                .put("adapter",adapter.name()).put("stage",stage.name()).put("outcome",outcome.name())
                .put("reason",reason.name()).put("elapsed_ms",elapsed()).put("stages",new JSONArray(stages.toString()));
            if(vendor!=null)j.put("usb_vendor_id",vendor).put("usb_product_id",product);
            String snapshot=j.toString();
            IO.execute(()->{try{
                if(!root.isDirectory()&&!root.mkdirs())return;
                FirmwareStore.writeJson(new File(root,id+".json"),new JSONObject(snapshot));
                File[] files=root.listFiles((d,n)->n.matches("[a-f0-9-]{36}\\.json"));
                if(files!=null){Arrays.sort(files,Comparator.comparingLong(File::lastModified).reversed());for(int i=20;i<files.length;i++)files[i].delete();}
            }catch(Exception ignored){} });
        }catch(Exception ignored){} // Reporting must never break a connection.
    }
    /** Worker-thread barrier before freezing a support report; never wait on the UI thread. */
    public static JSONArray collect(Context c) {
        JSONArray result=new JSONArray();
        try {
            IO.submit(()->{}).get(2,TimeUnit.SECONDS);
            File[] files=new File(c.getFilesDir(),"connection-attempts").listFiles((d,n)->n.matches("[a-f0-9-]{36}\\.json"));
            if(files==null)return result;
            Arrays.sort(files,Comparator.comparingLong(File::lastModified).reversed());
            for(int i=0;i<Math.min(20,files.length);i++)if(files[i].length()<16384){
                try {result.put(safe(FirmwareStore.json(files[i])));}catch(Exception ignored){}
            }
        }catch(Exception ignored){}
        return result;
    }
    static JSONObject safe(JSONObject raw)throws Exception {
        // Rebuild the export from typed fields; never forward an arbitrary JSON file.
        JSONObject j=new JSONObject().put("adapter",Adapter.valueOf(raw.getString("adapter")).name())
            .put("stage",Stage.valueOf(raw.getString("stage")).name())
            .put("outcome",Outcome.valueOf(raw.getString("outcome")).name())
            .put("reason",Reason.valueOf(raw.getString("reason")).name())
            .put("created_utc",java.time.Instant.parse(raw.getString("created_utc")).toString())
            .put("attempt_id",UUID.fromString(raw.getString("attempt_id")).toString())
            .put("elapsed_ms",Math.max(0,raw.getLong("elapsed_ms")));
        for(String key:new String[]{"usb_vendor_id","usb_product_id"})if(raw.has(key)){int n=raw.getInt(key);if(n>=0&&n<=65535)j.put(key,n);}
        JSONArray history=new JSONArray(),input=raw.optJSONArray("stages");
        if(input!=null)for(int i=0;i<Math.min(24,input.length());i++){
            JSONObject step=input.getJSONObject(i);
            history.put(new JSONObject().put("stage",Stage.valueOf(step.getString("stage")).name()).put("elapsed_ms",Math.max(0,step.getLong("elapsed_ms"))));
        }
        return j.put("stages",history);
    }
}
