// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.*;
import org.json.JSONObject;

/** Explicit operator workflow around original firmware collection; never an ECU client. */
public final class SecurityAccessView extends LinearLayout implements AutoCloseable {
    private static final AtomicBoolean WORKFLOW_BUSY=new AtomicBoolean();
    public static boolean workflowBusy(){return WORKFLOW_BUSY.get();}
    private static final String ENDPOINT="https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/process";
    private static final String INTERNET_REQUIRED="Security access requires an internet connection to the OpenSAAB security-access API. It cannot be processed offline.";
    private final Activity activity;
    private final boolean collection;
    private final Supplier<File> session;
    private final BooleanSupplier running;
    private final Runnable stop,collect,resume;
    private final TextView message;
    private final Button action;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final AtomicBoolean polling=new AtomicBoolean();
    private volatile boolean busy,closed;
    private volatile HttpURLConnection connection;
    private File observed;
    private boolean transferSeen,imported;
    private String failure;

    public SecurityAccessView(Activity activity,boolean collection,Supplier<File> session,
            BooleanSupplier running,Runnable stop,Runnable collect,Runnable resume){
        super(activity);this.activity=activity;this.collection=collection;this.session=session;
        this.running=running;this.stop=stop;this.collect=collect;this.resume=resume;
        setOrientation(VERTICAL);
        message=new TextView(activity);message.setTextSize(14);addView(message);
        action=new Button(activity);action.setTextSize(13);action.setOnClickListener(v->activate());addView(action);
        setVisibility(GONE);
    }
    public boolean busy(){return busy;}
    public void refresh(){
        if(closed||busy||!polling.compareAndSet(false,true))return;
        final File run=session.get();
        try{worker.execute(()->{
            String text="";
            try{if(run!=null){File f=new File(run,"native-dtc-screen.txt");if(f.isFile()&&f.length()<8192)text=new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8);}}catch(IOException ignored){}
            final String screen=text;
            activity.runOnUiThread(()->{
                polling.set(false);if(closed||busy||session.get()!=run)return;
                if(observed!=run){observed=run;transferSeen=false;imported=false;failure=null;}
                boolean prompt=SsaData.needsAccess(screen);
                if(collection&&prompt&&running.getAsBoolean())transferSeen=true;
                setVisibility(collection||prompt?VISIBLE:GONE);
                if(imported){show("Security data loaded. Return to the firmware and repeat your task; the vehicle still verifies access.","Return to firmware");return;}
                if(failure!=null){show(failure,collection&&transferSeen?"Retry processing":"Retry collection");return;}
                if(!collection){show("This task needs security access. Collect fresh data using the original firmware.\n\n"+INTERNET_REQUIRED,"Get security access");return;}
                if(transferSeen){show("Firmware reached the TIS transfer prompt. Send the collected security data to OpenSAAB for processing.\n\n"+INTERNET_REQUIRED,"Process security data");return;}
                String hint="Select Diagnostics → All → Get Security Access. Follow the original key-position prompts.";
                if(screen.contains("LOCK position"))hint="Follow the firmware prompt to turn the key to LOCK.";
                if(screen.contains("Remove Ignition"))hint="Follow the firmware prompt to remove the key.";
                show(hint+"\n\n"+INTERNET_REQUIRED,"Waiting for collection");action.setEnabled(false);
            });
        });}catch(RejectedExecutionException e){polling.set(false);}
    }
    private void show(String text,String label){message.setText(text);action.setText(label);action.setEnabled(true);}
    private void activate(){
        if(closed||busy)return;
        if(imported){resume.run();return;}
        if(!collection||!transferSeen){
            new AlertDialog.Builder(activity).setTitle("Collect security data")
                .setMessage(INTERNET_REQUIRED+"\n\nEnd this session and start original-firmware security collection. Select your vehicle, then Diagnostics → All → Get Security Access. The app will offer API processing at the transfer prompt.")
                .setNegativeButton("Later",null).setPositiveButton("Start collection",(d,w)->begin(false)).show();
        }else begin(true);
    }
    private void begin(boolean process){
        if(!WORKFLOW_BUSY.compareAndSet(false,true))return;
        final File run=session.get();busy=true;failure=null;action.setEnabled(false);
        message.setText("Closing the firmware session…");stop.run();
        worker.execute(()->{
            boolean released=false;
            try{
                long deadline=SystemClock.elapsedRealtime()+15000;
                while(running.getAsBoolean()&&SystemClock.elapsedRealtime()<deadline&&!closed)Thread.sleep(50);
                if(closed)return;
                if(running.getAsBoolean())throw new IOException("Session did not close. Stop USB, then retry.");
                if(session.get()!=run)throw new IOException("Session changed; collect again.");
                if(!process){WORKFLOW_BUSY.set(false);released=true;activity.runOnUiThread(()->{busy=false;if(!closed)collect.run();});return;}
                if(run==null)throw new IOException("No collected session");
                try(FirmwareGate.Lease lease=FirmwareGate.change()){process(run);}
                activity.runOnUiThread(()->{busy=false;imported=true;if(!closed)refresh();});
            }catch(Exception e){
                SupportReports.recordError(activity,e,false);
                String reason=e.getMessage()==null?e.getClass().getSimpleName():e.getMessage();
                android.util.Log.w("OpenSaabSecurity","Security workflow stopped: "+reason);
                activity.runOnUiThread(()->{busy=false;failure=reason;if(!closed){show(reason,process?"Retry processing":"Retry collection");}});
            }finally{if(!released)WORKFLOW_BUSY.set(false);}
        });
    }
    private void process(File run)throws Exception{
        JSONObject snapshot=new JSONObject(new String(readBounded(new File(run,"native-security-snapshot.json"),8192),StandardCharsets.UTF_8));
        if(!"original-guest-memory".equals(snapshot.getString("origin")) || snapshot.getInt("card_offset")!=SsaData.OFFSET || snapshot.getInt("bytes")!=SsaData.SIZE
                || !snapshot.getBoolean("ssa_memory_flash_enabled") || snapshot.getLong("ssa_erases")<1 || snapshot.getLong("ssa_programmed_bytes")<SsaData.SIZE)
            throw new IOException("Firmware has not written fresh SSA data. Collect again and follow all key prompts.");
        byte[] before=readBounded(new File(run,"ssa-card-before.bin"),SsaData.SIZE);
        byte[] input=readBounded(new File(run,"ssa-card-after.bin"),SsaData.SIZE);
        if(Arrays.equals(before,input))throw new IOException("SSA unchanged; fresh collection required.");
        SsaData.validateInput(input);
        File card=new File(activity.getFilesDir(),"firmware/card.bin");
        SsaCardImport.verifyBaseline(card,before);
        String originalHash=SsaCardImport.hash(card);
        File evidence=new File(run,"security-api-"+UUID.randomUUID());if(!evidence.mkdir())throw new IOException("Cannot save security evidence");
        Files.write(new File(evidence,"pre-auth.bin").toPath(),input);
        JSONObject request=new JSONObject().put("REQUEST_VERSION",1).put("SSA_DATA",Base64.getEncoder().encodeToString(input));
        byte[] payload=request.toString().getBytes(StandardCharsets.UTF_8);
        Files.write(new File(evidence,"request.json").toPath(),payload);
        activity.runOnUiThread(()->{if(!closed)message.setText("Contacting the OpenSAAB security-access API… Keep your internet connection active.");});
        if(closed)throw new IOException("Security processing cancelled");
        HttpURLConnection http=(HttpURLConnection)new URL(ENDPOINT).openConnection();connection=http;
        byte[] raw;
        try{
            http.setConnectTimeout(15000);http.setReadTimeout(30000);http.setInstanceFollowRedirects(false);
            http.setRequestMethod("POST");http.setRequestProperty("Content-Type","application/json");http.setDoOutput(true);http.setFixedLengthStreamingMode(payload.length);
            try(OutputStream out=http.getOutputStream()){out.write(payload);}
            int status=http.getResponseCode();if(status!=200)throw new IOException("OpenSAAB returned HTTP "+status+"; data not imported.");
            try(InputStream in=http.getInputStream()){raw=bounded(in,1024*1024);}
        }catch(IOException e){
            throw new IOException("Couldn’t complete the request to the OpenSAAB security-access API. Check your internet connection and try again. If it persists, the service may be unavailable. No security data was imported. "+e.getMessage(),e);
        }finally{http.disconnect();connection=null;}
        Files.write(new File(evidence,"response.json").toPath(),raw);
        JSONObject reply=new JSONObject(new String(raw,StandardCharsets.UTF_8));
        byte[] after=Base64.getDecoder().decode(reply.getString("SSA_DATA"));SsaData.validateReply(input,after);
        Files.write(new File(evidence,"post-auth.bin").toPath(),after);
        // Only the original guest SSA data region changes, with a verified backup.
        String updatedHash=SsaCardImport.apply(card,before,after,originalHash,evidence,
            ()->!closed&&!running.getAsBoolean()&&session.get()==run);
        JSONObject report=new JSONObject().put("status","security_data_imported").put("endpoint",ENDPOINT).put("bytes",SsaData.SIZE)
            .put("source_session",run.getName()).put("original_card_sha256",originalHash).put("working_card_sha256",updatedHash)
            .put("ssa_input_sha256",sha(input)).put("ssa_output_sha256",sha(after)).put("vehicle_access_verified",false);
        Files.write(new File(evidence,"result.json").toPath(),report.toString(2).getBytes(StandardCharsets.UTF_8));
    }
    private static byte[] readBounded(File file,int maximum)throws IOException{
        try(InputStream in=new FileInputStream(file)){return bounded(in,maximum);}
    }
    private static byte[] bounded(InputStream in,int maximum)throws IOException{
        ByteArrayOutputStream out=new ByteArrayOutputStream();byte[] b=new byte[8192];int n;
        while((n=in.read(b))!=-1){if(out.size()+n>maximum)throw new IOException("Unexpected file/response size");out.write(b,0,n);}return out.toByteArray();
    }
    private static String sha(byte[] bytes)throws Exception{
        StringBuilder s=new StringBuilder();for(byte b:MessageDigest.getInstance("SHA-256").digest(bytes))s.append(String.format(Locale.ROOT,"%02x",b&255));return s.toString();
    }
    @Override public void close(){closed=true;HttpURLConnection active=connection;if(active!=null)active.disconnect();worker.shutdown();}
}
