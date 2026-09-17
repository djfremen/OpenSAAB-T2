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
    private static final String INTERNET_REQUIRED="Security access requires internet. OpenSAAB is contacted first; you can allow Bojer as a fallback if OpenSAAB is unavailable. It cannot be processed offline.";
    private final Activity activity;
    private boolean collection;
    public void returnedToFirmware(){manualCollection=false;collection=false;navigator=null;manualNavigation=true;transferSeen=false;}
    private final Supplier<File> session;
    private final BooleanSupplier running;
    private final Runnable stop,collect,resume;
    private IntPredicate menuKey;
    private SecurityMenuNavigator navigator;
    private boolean manualNavigation,manualCollection;
    public void setMenuKey(IntPredicate key){menuKey=key;}
    public void manualNavigation(){manualNavigation=true;if(navigator!=null)navigator.cancel();}
    private final TextView message, stateLabel;
    private final LinearLayout buttons;
    private SsaState cardState=SsaState.UNAVAILABLE;
    private final Button action;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final AtomicBoolean polling=new AtomicBoolean();
    private volatile boolean busy,closed;
    private volatile HttpURLConnection connection;
    private File observed;
    private boolean transferSeen,imported;
    private String failure;
    private SecurityAccessStatus receipt;
    private boolean receiptCardMatches,detailsAction;
    private File receiptFile(){return new File(activity.getNoBackupFilesDir(),"security-processing-status.properties");}

    public SecurityAccessView(Activity activity,boolean collection,Supplier<File> session,
            BooleanSupplier running,Runnable stop,Runnable collect,Runnable resume){
        super(activity);this.activity=activity;this.collection=collection;this.session=session;
        this.running=running;this.stop=stop;this.collect=collect;this.resume=resume;
        setOrientation(VERTICAL);
        setPadding(SessionStyle.dp(activity,10),SessionStyle.dp(activity,10),SessionStyle.dp(activity,10),SessionStyle.dp(activity,10));
        android.graphics.drawable.GradientDrawable panel=new android.graphics.drawable.GradientDrawable();
        panel.setColor(0xff152432);panel.setCornerRadius(SessionStyle.dp(activity,8));setBackground(panel);
        stateLabel=new TextView(activity);stateLabel.setTag("security-state");stateLabel.setTextSize(14);stateLabel.setTypeface(null,android.graphics.Typeface.BOLD);addView(stateLabel);
        message=new TextView(activity);message.setTag("security-message");message.setTextSize(13);message.setTextColor(0xffbdcbd8);
        LinearLayout.LayoutParams messageParams=new LinearLayout.LayoutParams(-1,-2);messageParams.topMargin=SessionStyle.dp(activity,4);addView(message,messageParams);
        buttons=new LinearLayout(activity);LinearLayout.LayoutParams buttonParams=new LinearLayout.LayoutParams(-1,SessionStyle.dp(activity,48));buttonParams.topMargin=SessionStyle.dp(activity,8);addView(buttons,buttonParams);
        action=new Button(activity);action.setTag("security-action");action.setOnClickListener(v->activate());buttons.addView(action,new LinearLayout.LayoutParams(0,-1,1));SessionStyle.row(buttons);
        setVisibility(GONE);
    }
    public void addResetAction(Runnable reset){
        Button clear=new Button(activity);clear.setText("Clear offset");clear.setOnClickListener(v->reset.run());
        clear.setContentDescription("Clear offset — collect fresh security data");buttons.addView(clear);SessionStyle.row(buttons);
    }
    public boolean busy(){return busy;}
    public boolean readyToProcess(){return collection&&transferSeen&&!imported&&!busy;}
    public void processCollected(){if(readyToProcess())activate();}
    public boolean collecting(){return collection&&!imported;}
    /** Follow collection in this full-control session without rebooting or re-selecting menus. */
    public void collectInCurrentSession(){
        collection=true;manualCollection=true;manualNavigation=true;navigator=null;transferSeen=false;imported=false;failure=null;
    }
    public boolean navigating(){return collection&&!busy&&navigator!=null&&navigator.active();}
    public void refresh(){
        if(closed||busy||!polling.compareAndSet(false,true))return;
        final File run=session.get();
        try{worker.execute(()->{
            String text="";
            try{if(run!=null){File f=new File(run,"native-dtc-screen.txt");if(f.isFile()&&f.length()<8192)text=new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8);}}catch(IOException ignored){}
            SecurityAccessStatus saved=SecurityAccessStatus.read(receiptFile());
            VehicleIdentity identity=run==null?null:VehicleSession.read(new File(run,VehicleSession.FILE));
            try{if(identity==null||saved==null||!saved.matches(identity.vin))saved=null;}catch(Exception invalid){saved=null;}
            final SecurityAccessStatus current=saved;
            final VehicleIdentity currentIdentity=identity;
            final boolean cardMatches=current!=null&&current.cardMatches(new File(activity.getFilesDir(),"firmware/card.bin"));
            final String screen=text;
            final SsaState currentState=SsaState.read(new File(activity.getFilesDir(),"firmware/card.bin"));
            activity.runOnUiThread(()->{
                polling.set(false);if(closed||busy||session.get()!=run)return;
                if(observed!=run){if(manualCollection)collection=false;observed=run;manualCollection=false;transferSeen=false;imported=false;failure=null;
                    navigator=collection&&run!=null?new SecurityMenuNavigator(currentIdentity):null;
                    if(manualNavigation&&navigator!=null)navigator.cancel();}
                receipt=current;receiptCardMatches=cardMatches;detailsAction=false;
                cardState=currentState;
                String age=receipt==null?"Age unknown":receipt.freshness(cardMatches,java.time.Instant.now());
                stateLabel.setText("Security status  "+cardState.label+(cardState==SsaState.POST_AUTH?" · "+age:""));
                stateLabel.setTextColor(cardState==SsaState.POST_AUTH&&"Stale".equals(age)?0xffffd77c:cardState.color);
                boolean prompt=SsaData.needsAccess(screen);
                if(!collection&&run!=null&&currentIdentity!=null&&running.getAsBoolean()&&SsaData.collectingAccess(screen)){
                    collectInCurrentSession();android.util.Log.i("OpenSaabSecurity","Manual security collection detected in current firmware session");
                }
                if(collection&&!transferSeen&&SsaData.collectingAccess(screen)){
                    stateLabel.setText("Security status · Collecting pre-auth");stateLabel.setTextColor(SsaState.PRE_AUTH.color);
                }
                if(collection&&prompt&&running.getAsBoolean())transferSeen=true;
                if(collection&&transferSeen){stateLabel.setText("Security status · Ready to process");stateLabel.setTextColor(SsaState.PRE_AUTH.color);}
                setVisibility(VISIBLE);
                imported=!manualCollection&&receipt!=null&&receipt.sameSession(run)&&receipt.imported()&&cardMatches;
                if(imported){show(receipt==null?"Security data loaded.":receipt.summary(true,running.getAsBoolean(),cardMatches),"Return to firmware");message.setOnClickListener(v->showReceipt());return;}
                if(failure!=null){show(failure,collection&&transferSeen?"Retry processing":"Retry collection");return;}
                if(!collection&&!prompt&&receipt!=null){
                    show(receipt.summary(receipt.sameSession(run),running.getAsBoolean(),cardMatches),"Details");
                    detailsAction=true;return;
                }
                if(!collection&&!prompt){show(cardState.explanation,"Details");detailsAction=true;return;}
                if(!collection){show("This task needs security access. Collect fresh data using the original firmware.\n\n"+INTERNET_REQUIRED,"Get security access");return;}
                if(transferSeen){show("Firmware reached the TIS transfer prompt. Send the collected security data to OpenSAAB for processing.\n\n"+INTERNET_REQUIRED,"Process security data");return;}
                String hint=manualNavigation?"Security collection detected in this session. Follow the firmware’s ignition-key prompts.":"Select Diagnostics → All → Get Security Access. Follow the original key-position prompts.";
                if(navigator!=null){
                    long now=SystemClock.elapsedRealtime();
                    if(running.getAsBoolean()&&menuKey!=null){Integer key=navigator.next(screen,now);if(key!=null&&menuKey.test(key))navigator.sent(now);}
                    hint=navigator.hint();
                }
                if(screen.contains("LOCK position"))hint="Follow the firmware prompt to turn the key to LOCK.";
                if(screen.contains("Remove Ignition"))hint="Follow the firmware prompt to remove the key.";
                show(hint+"\n\n"+INTERNET_REQUIRED,"Waiting for collection");action.setEnabled(false);
            });
        });}catch(RejectedExecutionException e){polling.set(false);}
    }
    private void show(String text,String label){message.setText(text);message.setOnClickListener(null);action.setText(label);action.setEnabled(true);}
    private void showReceipt(){new AlertDialog.Builder(activity).setTitle("Security status · "+cardState.label)
        .setMessage(cardState.explanation+"\n\n"+(receipt==null?"No processing history for this vehicle. Data age is unknown.":receipt.details(running.getAsBoolean(),receiptCardMatches)))
        .setPositiveButton("Done",null).show();}
    private void activate(){
        if(closed||busy)return;
        if(detailsAction){showReceipt();return;}
        if(imported){resume.run();return;}
        if(!collection||!transferSeen){
            new AlertDialog.Builder(activity).setTitle("Collect security data")
                .setMessage(INTERNET_REQUIRED+"\n\nEnd this session and start original-firmware security collection. OpenSAAB will select the identified vehicle’s year and platform, then All → Get Security Access when those menus are recognized. Follow the firmware’s ignition-key prompts. The app will offer API processing at the transfer prompt.")
                .setNegativeButton("Later",null).setPositiveButton("Start collection",(d,w)->begin(false,false)).show();
        }else{
            if(!SecurityAuthorization.available(activity)){SecurityAuthorization.show(activity,this::activate);return;}
            new AlertDialog.Builder(activity).setTitle("Share VIN and process security data")
                .setMessage("Your VIN will be shared with OpenSAAB along with the collected 714-byte security file. It is stored privately for processing and troubleshooting, with deletion scheduled after one day. Only authorized OpenSAAB operators can access it. Contact OpenSAAB for deletion. Request outcome/timing records are kept for seven days. No processed response is archived by this service. If OpenSAAB is unavailable, Allow fallback also permits sending the same data to Bojer at sas.mysaab.info, a separate service.\n\n")
                .setNegativeButton("Cancel",null)
                .setNeutralButton("OpenSAAB only",(d,w)->begin(true,false))
                .setPositiveButton("Allow fallback",(d,w)->begin(true,true)).show();
        }
    }
    private void begin(boolean process,boolean allowFallback){
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
                try(FirmwareGate.Lease lease=FirmwareGate.change()){process(run,allowFallback);}
                // Release the image lease and workflow gate before the new emulator opens the card.
                WORKFLOW_BUSY.set(false);released=true;
                activity.runOnUiThread(()->{
                    busy=false;manualCollection=false;imported=true;
                    if(!closed&&session.get()==run){
                        show("[POST-AUTH] — restarting firmware…","Restarting firmware");action.setEnabled(false);
                        android.widget.Toast.makeText(activity,"[POST-AUTH] — restarting firmware",android.widget.Toast.LENGTH_LONG).show();
                        resume.run();
                    }
                });
            }catch(Exception e){
                SupportReports.recordError(activity,e,false);
                String reason=e.getMessage()==null?e.getClass().getSimpleName():e.getMessage();
                android.util.Log.w("OpenSaabSecurity","Security workflow stopped: "+reason);
                activity.runOnUiThread(()->{busy=false;failure=reason;if(!closed){show(reason,process?"Retry processing":"Retry collection");}});
            }finally{if(!released)WORKFLOW_BUSY.set(false);}
        });
    }
    private void process(File run,boolean allowFallback)throws Exception{
        VehicleIdentity identity=VehicleSession.read(new File(run,VehicleSession.FILE));
        if(identity==null)throw new IOException("No verified vehicle identity; collect again.");
        SecurityAccessStatus attempt=new SecurityAccessStatus(identity.vin,run.getName(),java.time.Instant.now());
        attempt.save(receiptFile());
        try{processAttempt(run,allowFallback,identity,attempt);}
        catch(Exception failure){
            attempt.failed(java.time.Instant.now());
            try{attempt.save(receiptFile());}catch(Exception persistence){failure.addSuppressed(persistence);}
            throw failure;
        }
    }
    private void processAttempt(File run,boolean allowFallback,VehicleIdentity identity,SecurityAccessStatus attempt)throws Exception{
        JSONObject snapshot=new JSONObject(new String(readBounded(new File(run,"native-security-snapshot.json"),8192),StandardCharsets.UTF_8));
        if(!"original-guest-memory".equals(snapshot.getString("origin")) || snapshot.getInt("card_offset")!=SsaData.OFFSET || snapshot.getInt("bytes")!=SsaData.SIZE
                || !snapshot.getBoolean("ssa_memory_flash_enabled") || snapshot.getLong("ssa_erases")<1 || snapshot.getLong("ssa_programmed_bytes")<SsaData.SIZE)
            throw new IOException("Firmware has not written fresh SSA data. Collect again and follow all key prompts.");
        byte[] before=readBounded(new File(run,"ssa-card-before.bin"),SsaData.SIZE);
        byte[] input=readBounded(new File(run,"ssa-card-after.bin"),SsaData.SIZE);
        if(Arrays.equals(before,input))throw new IOException("SSA unchanged; fresh collection required.");
        SsaData.validateInput(input);
        attempt.collected(java.time.Instant.now());attempt.save(receiptFile());
        if(!identity.vin.equals(SsaData.vin(input)))throw new IOException("Collected security data belongs to a different vehicle; collect again.");
        File card=new File(activity.getFilesDir(),"firmware/card.bin");
        SsaCardImport.verifyBaseline(card,before);
        String originalHash=SsaCardImport.hash(card);
        File evidence=new File(run,"security-api-"+UUID.randomUUID());if(!evidence.mkdir())throw new IOException("Cannot save security evidence");
        Files.write(new File(evidence,"pre-auth.bin").toPath(),input);
        JSONObject request=new JSONObject().put("REQUEST_VERSION",1).put("SSA_DATA",Base64.getEncoder().encodeToString(input));
        byte[] payload=request.toString().getBytes(StandardCharsets.UTF_8);
        Files.write(new File(evidence,"request.json").toPath(),payload);
        SecurityApiClient.Result result=SecurityApiClient.process(payload,allowFallback,
            ()->closed||session.get()!=run,this::postSecurityData);
        byte[] raw=result.body;
        Files.write(new File(evidence,"response.json").toPath(),raw);
        JSONObject reply=new JSONObject(new String(raw,StandardCharsets.UTF_8));
        byte[] after=Base64.getDecoder().decode(reply.getString("SSA_DATA"));SsaData.validateReply(input,after);
        attempt.processed(SecurityApiClient.FALLBACK.equals(result.endpoint)?"Bojer":"OpenSAAB",reply.optString("request_id",""),java.time.Instant.now());
        attempt.save(receiptFile());
        Files.write(new File(evidence,"post-auth.bin").toPath(),after);
        // Only the original guest SSA data region changes, with a verified backup.
        String updatedHash=SsaCardImport.apply(card,before,after,originalHash,evidence,
            ()->!closed&&!running.getAsBoolean()&&session.get()==run);
        attempt.imported(after,java.time.Instant.now());attempt.save(receiptFile());
        JSONObject report=new JSONObject().put("started_utc",attempt.data.getProperty("started_utc"))
            .put("server_reply_validated_utc",attempt.data.getProperty("processed_utc"))
            .put("imported_utc",attempt.data.getProperty("imported_utc"))
            .put("request_id",attempt.data.getProperty("request_id",""))
            .put("vehicle_access_granted_utc",JSONObject.NULL)
            .put("status","security_data_imported").put("endpoint",result.endpoint)
            .put("fallback_allowed",allowFallback).put("fallback_reason",result.fallbackReason).put("bytes",SsaData.SIZE)
            .put("source_session",run.getName()).put("original_card_sha256",originalHash).put("working_card_sha256",updatedHash)
            .put("ssa_input_sha256",sha(input)).put("ssa_output_sha256",sha(after)).put("vehicle_access_verified",false);
        Files.write(new File(evidence,"result.json").toPath(),report.toString(2).getBytes(StandardCharsets.UTF_8));
    }
    private SecurityApiClient.Response postSecurityData(String endpoint,byte[] payload)throws IOException{
        boolean fallback=SecurityApiClient.FALLBACK.equals(endpoint);
        activity.runOnUiThread(()->{if(!closed)message.setText(fallback
            ?"OpenSAAB is unavailable. Contacting Bojer using the fallback you allowed…"
            :"Contacting the OpenSAAB security-access API… Keep your internet connection active.");});
        if(closed)throw new IOException("Security processing cancelled");
        HttpURLConnection http=(HttpURLConnection)new URL(endpoint).openConnection();connection=http;
        try{
            if(closed)throw new IOException("Security processing cancelled");
            http.setConnectTimeout(15000);http.setReadTimeout(30000);http.setInstanceFollowRedirects(false);
            if(!fallback){http.setRequestProperty("Authorization","Bearer "+SecurityAuthorization.bearer(activity));http.setRequestProperty("X-OpenSAAB-Consent","security-storage-24h-v1");}
            http.setRequestMethod("POST");http.setRequestProperty("Content-Type","application/json");http.setDoOutput(true);http.setFixedLengthStreamingMode(payload.length);
            try(OutputStream out=http.getOutputStream()){out.write(payload);}
            int status=http.getResponseCode();
            if(!fallback&&status==401){SecurityAuthorization.forget(activity);throw new IOException("Incorrect security access password. Tap Retry processing to enter it again; case matters.");}
            if(status!=200)return new SecurityApiClient.Response(status,new byte[0]);
            try(InputStream in=http.getInputStream()){return new SecurityApiClient.Response(status,bounded(in,1024*1024));}
        }finally{http.disconnect();connection=null;}
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
