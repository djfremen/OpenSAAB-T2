// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.os.SystemClock;
import java.io.*;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import org.json.JSONObject;

/** Holds only a protocol-clean SPP socket while the stopped run's card is imported. */
public final class MxSecurityTransport implements AutoCloseable {
    public static final long MAX_HOLD_MS=5*60*1000;
    public static final class Binding {
        public final File run,card;
        public final String device,vin,beforeHash;
        public final boolean compatibility;
        public Binding(File run,File card,String device,boolean compatibility,String vin)throws Exception{
            this.run=run.getCanonicalFile();this.card=card.getCanonicalFile();this.device=device;this.compatibility=compatibility;this.vin=vin;
            beforeHash=null;
        }
        private Binding(Binding context,String hash){run=context.run;card=context.card;device=context.device;compatibility=context.compatibility;vin=context.vin;beforeHash=hash;}
        /** Call only after the firmware child has actually exited, while its image lease is held. */
        public Binding stoppedCard()throws Exception{return new Binding(this,SsaCardImport.hash(card));}
    }
    private final Binding binding;
    private final VlinkerSppTransport transport;
    private final SecurityTransportLease<VlinkerSppTransport> lease;
    private final Context context;
    private final Thread worker;
    private volatile Process core;
    private volatile boolean closed,quiescing,verified,coreDead,transferred;
    private volatile String failure;
    private BufferedWriter trace;
    private InputStream input;
    private OutputStream output;
    MxSecurityTransport(Context context,Binding binding,VlinkerSppTransport transport,
            boolean processDead,boolean cleanExit,boolean protocolClean)throws Exception{
        this.context=context;this.binding=binding;this.transport=transport;
        lease=new SecurityTransportLease<>(binding.run.toString(),binding.card.toString(),binding.device,binding.compatibility,binding.vin,binding.beforeHash,
                SystemClock.elapsedRealtime()+MAX_HOLD_MS,SystemClock::elapsedRealtime,transport,VlinkerSppTransport::shutdown,processDead,cleanExit,protocolClean);
        try{trace=new BufferedWriter(new OutputStreamWriter(new FileOutputStream(new File(binding.run,"security-transport.jsonl")),StandardCharsets.UTF_8));}
        catch(Exception error){lease.close();throw error;}
        worker=new Thread(this::run,"mx-security-transport");worker.setDaemon(true);worker.start();
    }
    private synchronized void record(String event,JSONObject detail)throws Exception{
        trace.write(new JSONObject().put("event",event).put("elapsed_realtime_ms",SystemClock.elapsedRealtime()).put("data",detail).toString());trace.newLine();trace.flush();
    }
    private JSONObject event(String op,JSONObject detail)throws Exception{
        detail.put("scope","mx_idle").put("op",op);
        output.write((detail+"\n").getBytes(StandardCharsets.UTF_8));output.flush();
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();int b;
        long deadline=SystemClock.elapsedRealtime()+2000;
        while(true){
            if(closed)throw new IOException("Retained transport cancelled");
            if(input.available()==0){
                if(!core.isAlive())throw new IOException("Retained transport protocol worker stopped");
                if(SystemClock.elapsedRealtime()>=deadline)throw new IOException("Retained transport protocol worker response timed out");
                Thread.sleep(5);continue;
            }
            b=input.read();if(b<0)throw new IOException("Retained transport protocol worker stopped");if(b==10)break;
            if(bytes.size()>16384)throw new IOException("Retained transport response exceeded limit");bytes.write(b);
        }
        JSONObject state=new JSONObject(bytes.toString("UTF-8"));record("core_effect",state);return state;
    }
    private void run(){
        try{
            record("retained",new JSONObject().put("firmware_process_dead",true).put("protocol_cleanup","succeeded").put("bluetooth_closed",false).put("expires_after_ms",MAX_HOLD_MS));
            core=new ProcessBuilder(new File(context.getApplicationInfo().nativeLibraryDir,"libopensaab_connection.so").toString(),context.getFilesDir().toString())
                    .redirectError(new File(binding.run,"security-transport-core.log")).start();
            input=core.getInputStream();output=core.getOutputStream();
            JSONObject state=event("start",new JSONObject().put("owner",binding.run.getName()));
            while(!closed){
                if(!lease.available())throw new IOException("Retained Bluetooth connection expired");
                String phase=state.optString("state");
                if(phase.equals("failed"))throw new IOException(state.optString("error","Retained connection readiness failed"));
                if(phase.equals("send")){
                    verified=false;
                    if(!state.optString("command").equals("ATI"))throw new IOException("Unexpected retained transport command");
                    record("adapter_probe",new JSONObject().put("command","ATI").put("vehicle_request",false));
                    transport.send(new JSONObject().put("op","write").put("command","ATI"));
                    state=new JSONObject().put("state","waiting");
                }else if(!phase.equals("waiting"))throw new IOException("Unexpected retained transport state");
                else if(quiescing&&verified){
                    // RX can arrive before the synchronous write-completion receipt.
                    // Drain only those receipts; unsolicited bytes remain a closed-protocol failure.
                    try{
                        JSONObject pending=transport.receive(1);
                        if(pending.optString("type").equals("write_completed"))continue;
                        if(!pending.optString("type").equals("rx"))throw new IOException("Retained Bluetooth transport ended during handoff");
                        record("adapter_rx",pending);state=event("rx",new JSONObject().put("bytes",pending.getJSONArray("bytes")));verified=state.optBoolean("ready_for_transfer",false);continue;
                    }catch(SocketTimeoutException quiet){break;}
                }
                try{
                    JSONObject packet=transport.receive(100);
                    if(packet.optString("type").equals("write_completed"))continue;
                    if(!packet.optString("type").equals("rx"))throw new IOException("Retained Bluetooth transport ended");
                    record("adapter_rx",packet);state=event("rx",new JSONObject().put("bytes",packet.getJSONArray("bytes")));
                    verified=state.optBoolean("ready_for_transfer",false);
                }catch(SocketTimeoutException waiting){state=event("tick",new JSONObject());verified=state.optBoolean("ready_for_transfer",false);}
            }
        }catch(Exception error){failure=String.valueOf(error.getMessage());try{record("failed",new JSONObject().put("reason",failure));}catch(Exception ignored){};}
        finally{
            Process child=core;if(child!=null){
                try{if(output!=null)output.close();}catch(Exception ignored){}
                NativeProcessExit.Result exit=NativeProcessExit.await(child,500);coreDead=!child.isAlive();
                try{record("worker_exit",new JSONObject().put("process_dead",coreDead).put("forced",exit.forced).put("exit_code",exit.exitCode==null?JSONObject.NULL:exit.exitCode));}catch(Exception ignored){}
                if(exit.forced||exit.exitCode==null||exit.exitCode!=0)failure="Retained transport protocol worker did not exit cleanly";
            }
            if(closed||failure!=null||!quiescing)lease.close();
            try{record("idle_owner_ended",new JSONObject().put("ready_for_transfer",quiescing&&verified&&failure==null&&coreDead&&!closed&&lease.available()&&transport.readyForHandoff()).put("cancelled",closed).put("prompt_verified",verified).put("failure",failure==null?JSONObject.NULL:failure).put("transport",transport.connectionEvidence()));}catch(Exception ignored){}
            try{if(trace!=null)trace.close();}catch(Exception ignored){}
        }
    }
    public boolean available(){return !closed&&failure==null&&lease.available();}
    public void imported(File run,File card,String beforeHash,String afterHash)throws Exception{
        lease.imported(run.getCanonicalPath(),card.getCanonicalPath(),beforeHash,afterHash);
    }
    /** Worker thread only: finish the in-flight harmless probe before transferring UART ownership. */
    public MxNativeBridge.PreparedTransport claim(File run,File card,String device,boolean compatibility,String vin,String currentHash)throws Exception{
        if(!available()){close();throw new IOException(failure==null?"Retained connection unavailable":failure);}
        quiescing=true;worker.join(5000);
        if(worker.isAlive()||!coreDead||core==null||core.isAlive()||failure!=null||!verified||!transport.readyForHandoff()){
            close();throw new IOException("Retained connection did not reach a clean prompt; reconnect explicitly");
        }
        VlinkerSppTransport value=lease.claim(run.getCanonicalPath(),card.getCanonicalPath(),device,compatibility,vin,currentHash);
        try{
            JSONObject receipt=new JSONObject().put("schema",1).put("state","transferred_for_fresh_vin").put("socket_reopened",false)
                .put("elapsed_realtime_ms",SystemClock.elapsedRealtime()).put("idle_worker_dead",coreDead).put("prompt_verified",verified)
                .put("same_run_card_device_vin",true).put("imported_card_sha256",currentHash).put("automatic_replay",false).put("workflow_qualified",false);
            Files.write(new File(binding.run,"security-transport-handoff.json").toPath(),receipt.toString(2).getBytes(StandardCharsets.UTF_8));
        }catch(Exception evidenceFailure){value.shutdown();closed=true;throw evidenceFailure;}
        transferred=true;closed=true;
        return new MxNativeBridge.PreparedTransport("security-"+binding.run.getName(),device,compatibility,value);
    }
    public void close(){
        if(transferred)return;
        closed=true;lease.close();Process child=core;if(child!=null)child.destroy();
        if(Thread.currentThread()!=worker)try{worker.join(2000);}catch(InterruptedException error){Thread.currentThread().interrupt();}
    }
}
