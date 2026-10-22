// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Activity;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** Fixed working-card operation; no arbitrary offsets, file paths or vehicle commands. */
final class McpSecurityCard {
    private static volatile Job job;
    private static final class Job {
        final String id,hash;final long generation;final AtomicBoolean cancelled=new AtomicBoolean(),committed=new AtomicBoolean();
        volatile String status="running",error;volatile JSONObject result;
        Job(String id,String hash,long generation){this.id=id;this.hash=hash;this.generation=generation;}
        JSONObject json()throws Exception{
            JSONObject value=new JSONObject().put("request_id",id).put("status",status).put("card_changed",committed.get()).put("vehicle_command_sent",false);
            if(result!=null)value.put("result",new JSONObject(result.toString()));
            if(error!=null)value.put("error",error);
            return value;
        }
    }
    static JSONObject inspect(Activity a,long generation,boolean idle)throws Exception{
        Job current=job;
        JSONObject value=new JSONObject().put("manual_generation",generation).put("reset_available",idle&&(current==null||!current.status.equals("running"))&&!SecurityReset.busy()&&!FirmwareGate.sessionActive()&&!SecurityAccessView.workflowBusy());
        if(!value.getBoolean("reset_available"))return value.put("reason","Stop firmware and wait for adapter/security cleanup");
        try(FirmwareGate.Lease lease=FirmwareGate.change()){
            File card=new File(a.getFilesDir(),"firmware/card.bin");
            if(card.length()!=33554432)return value.put("reset_available",false).put("reason","Valid 32 MiB working card unavailable");
            return value.put("card_state",SsaState.read(card).name()).put("card_sha256",SsaCardImport.hash(card)).put("card_bytes",card.length());
        }
    }
    static synchronized JSONObject start(Activity a,JSONObject request,long generation,boolean idle,BooleanSupplier permission)throws Exception{
        String id=request.optString("request_id"),hash=request.optString("expected_card_sha256");
        if(!id.matches("[A-Za-z0-9][A-Za-z0-9_-]{7,79}")||!hash.matches("[0-9a-f]{64}")||!"RESET_TO_INIT_AUTH".equals(request.optString("confirmation")))throw new IOException("Explicit reset confirmation, request ID and inspected card hash required");
        if(request.optLong("manual_generation",-1)!=generation)throw new IOException("Manual takeover; inspect the card again");
        File retained=record(a,id);
        if(retained.isFile()){
            JSONObject previous=read(retained);
            if(!hash.equals(previous.optString("expected_card_sha256"))||generation!=previous.optLong("manual_generation",-1))throw new IOException("Request ID already bound to a different reset");
            if(job==null||!job.id.equals(id))return historical(previous);
        }
        if(job!=null&&job.id.equals(id)){
            if(!job.hash.equals(hash)||job.generation!=generation)throw new IOException("Request ID already bound to a different reset");
            return job.json();
        }
        if(!idle||FirmwareGate.sessionActive()||SecurityReset.busy()||SecurityAccessView.workflowBusy()||(job!=null&&job.status.equals("running")))throw new IOException("Stop firmware and wait for adapter/security cleanup");
        final Job current=new Job(id,hash,generation);job=current;
        try{save(a,current);}catch(Exception e){job=null;throw e;}
        final android.app.ProgressDialog progress=new android.app.ProgressDialog(a);
        try{progress.setMessage("MCP: backing up and resetting working card to INIT_AUTH…");progress.setCancelable(false);progress.show();}
        catch(Exception e){current.error=e.getMessage();current.status="failed";save(a,current);throw e;}
        new Thread(()->{
            try{
                current.result=SecurityReset.perform(a,()->false,()->permission.getAsBoolean()&&!current.cancelled.get(),hash,current.committed);
                current.status="completed";
            }catch(Exception e){current.error=e.getMessage();current.status=current.committed.get()?"changed_metadata_or_readback_failed":current.cancelled.get()?"cancelled":"failed";}
            finally{
                try{save(a,current);}catch(Exception e){current.error="Reset outcome persistence failed: "+e.getMessage();current.status=current.committed.get()?"changed_metadata_or_readback_failed":"failed";}
                a.runOnUiThread(()->{if(!a.isDestroyed())progress.dismiss();});
                AndroidMcp.changed();
            }
        },"mcp-security-card-reset").start();
        return current.json();
    }
    static synchronized JSONObject status(Activity a,String id,boolean cancel)throws Exception{
        if(!id.matches("[A-Za-z0-9][A-Za-z0-9_-]{7,79}"))throw new IOException("Invalid reset request ID");
        if(job==null||!job.id.equals(id)){
            File file=record(a,id);
            if(file.isFile())return historical(read(file));
            throw new IOException("Unknown reset request ID; no operation replayed");
        }
        if(cancel&&job.status.equals("running"))job.cancelled.set(true);
        return job.json();
    }
    private static File record(Activity a,String id){return new File(a.getFilesDir(),"mcp-card-resets/"+id+".json");}
    private static JSONObject read(File file)throws Exception{return new JSONObject(new String(Files.readAllBytes(file.toPath()),StandardCharsets.UTF_8));}
    private static JSONObject historical(JSONObject value)throws Exception{
        if(value.optString("status").equals("running"))value.put("status","interrupted_unverified").put("card_changed",JSONObject.NULL);
        return value.put("replayed",false);
    }
    private static void save(Activity a,Job current)throws Exception{
        JSONObject value=current.json().put("expected_card_sha256",current.hash).put("manual_generation",current.generation);
        File file=record(a,current.id);Files.createDirectories(file.getParentFile().toPath());
        Path temp=Files.createTempFile(file.getParentFile().toPath(),"reset-",".tmp");
        try{Files.write(temp,value.toString(2).getBytes(StandardCharsets.UTF_8));Files.move(temp,file.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);}finally{Files.deleteIfExists(temp);}
    }
    private McpSecurityCard(){}
}
