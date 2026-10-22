// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.net.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.*;import java.util.Arrays;import org.json.*;
/** Real authenticated endpoint and production card transaction, on a disposable emulator only. */
public final class McpSecurityCardInstrumentedTest extends Instrumentation {
    private Bundle arguments;private String token;private int checks;
    public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
    private void check(boolean condition,String message){checks++;if(!condition)throw new AssertionError(message);}
    private static final class Owner implements AndroidMcp.Owner {
        volatile boolean running;
        public JSONObject state()throws Exception{return new JSONObject().put("session","fixture").put("session_running",running).put("busy",false).put("input_idle",true);}
        public boolean key(int code){throw new AssertionError("Card reset sent a firmware key");}public void startOffline(){throw new AssertionError("Card reset started firmware");}public void stop(){running=false;}
    }
    private JSONObject call(String op,JSONObject args)throws Exception{
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(5000);
            args.put("token",token).put("op",op);socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));
            ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=socket.getInputStream().read())!=-1&&n!=10)b.write(n);
            if(b.size()==0)throw new EOFException("No authenticated response");return new JSONObject(b.toString("UTF-8"));
        }
    }
    private JSONObject request(String id,String hash,long generation)throws Exception{return new JSONObject().put("request_id",id).put("expected_card_sha256",hash).put("manual_generation",generation).put("confirmation","RESET_TO_INIT_AUTH");}
    private JSONObject waitFor(String id)throws Exception{
        long deadline=SystemClock.elapsedRealtime()+30000;JSONObject result;
        do{result=call("security_card_status",new JSONObject().put("request_id",id));if(!"running".equals(result.optString("status")))return result;Thread.sleep(30);}while(SystemClock.elapsedRealtime()<deadline);
        throw new AssertionError("Reset did not finalize: "+result);
    }
    public void onStart(){Activity a=null;Bundle result=new Bundle();int code=-1;
        try{
            DisposableEmulator.require(getTargetContext());DisposableEmulator.requireInstalledVersion(getTargetContext(),arguments);
            File card=new File(getTargetContext().getFilesDir(),"firmware/card.bin");card.getParentFile().mkdirs();
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.setLength(33554432);f.seek(SsaData.OFFSET);byte[] original=new byte[SsaData.SIZE];Arrays.fill(original,(byte)'A');f.write(original);}
            final String before=SsaCardImport.hash(card);
            a=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity host=a;
            runOnMainSync(()->{android.view.View welcome=host.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(welcome!=null)welcome.performClick();});waitForIdleSync();
            Owner owner=new Owner();runOnMainSync(()->AndroidMcp.bind(host,owner));
            token=new String(Files.readAllBytes(new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            JSONObject initial=call("security_card",new JSONObject());long generation=initial.getLong("manual_generation");
            check(initial.getBoolean("reset_available")&&before.equals(initial.getString("card_sha256")),"Initial card hash unavailable");
            check(initial.getString("card_state").equals("POST_AUTH"),"Fixture state not recognized");
            check(call("security_card_reset",request("missing-confirm",before,generation).put("confirmation","wrong")).has("error"),"Missing explicit confirmation accepted");
            check(call("security_card_reset",request("stale-generation",before,generation+1)).has("error"),"Stale manual generation accepted");
            runOnMainSync(()->owner.running=true);
            check(!call("security_card",new JSONObject()).getBoolean("reset_available"),"Active-session reset advertised");
            check(call("security_card_reset",request("running-session",before,generation)).has("error"),"Active session accepted");
            runOnMainSync(()->owner.running=false);
            try(FirmwareGate.Lease lease=FirmwareGate.use()){
                check(call("security_card_reset",request("cleanup-still-active",before,generation)).has("error"),"Unreleased firmware/USB lease accepted");
            }
            String staleId="stale-hash-"+SystemClock.elapsedRealtime();
            call("security_card_reset",request(staleId,"0".repeat(64),generation));
            JSONObject stale=waitFor(staleId);check(stale.getString("status").equals("failed")&&!stale.getBoolean("card_changed"),"Stale card was changed");
            check(SsaCardImport.hash(card).equals(before),"Rejected requests changed card");
            String id="card-reset-"+SystemClock.elapsedRealtime();JSONObject request=request(id,before,generation);
            call("security_card_reset",new JSONObject(request.toString()));JSONObject done=waitFor(id);
            check(done.getString("status").equals("completed")&&done.getBoolean("card_changed"),"Reset did not complete: "+done);
            JSONObject evidence=done.getJSONObject("result");
            check(evidence.getString("card_state").equals("INIT_AUTH")&&!evidence.getBoolean("vehicle_command_sent"),"Reset result scope wrong");
            check(evidence.getString("backup_sha256").equals(before),"Full backup hash wrong");
            check(evidence.getInt("offset")==0xfe0000&&evidence.getInt("bytes")==714&&evidence.getInt("fill")==255,"Reset bounds changed");
            JSONObject after=call("security_card",new JSONObject());
            check(after.getString("card_state").equals("INIT_AUTH")&&after.getString("card_sha256").equals(evidence.getString("working_card_sha256")),"Readback differs");
            JSONObject duplicate=call("security_card_reset",new JSONObject(request.toString()));
            check(duplicate.getJSONObject("result").getString("receipt_relative_path").equals(evidence.getString("receipt_relative_path")),"Repeated ID replayed reset");
            java.lang.reflect.Field retained=McpSecurityCard.class.getDeclaredField("job");retained.setAccessible(true);retained.set(null,null);
            JSONObject recovered=call("security_card_status",new JSONObject().put("request_id",id));
            check(recovered.getString("status").equals("completed"),"Persisted result lost after controller state reset");
            check(call("security_card_reset",new JSONObject(request.toString())).getJSONObject("result").getString("receipt_relative_path").equals(evidence.getString("receipt_relative_path")),"Persisted request replayed");
            check(call("security_card_status",new JSONObject().put("request_id","unknown-reset")).has("error"),"Unknown ID manufactured success");
            try(RandomAccessFile f=new RandomAccessFile(card,"rw")){f.seek(SsaData.OFFSET);byte[] original=new byte[SsaData.SIZE];Arrays.fill(original,(byte)'A');f.write(original);}
            JSONObject cancelBefore=call("security_card",new JSONObject());String cancelHash=cancelBefore.getString("card_sha256"),cancelId="cancel-reset-"+SystemClock.elapsedRealtime();
            call("security_card_reset",request(cancelId,cancelHash,generation));
            call("security_card_cancel",new JSONObject().put("request_id",cancelId));JSONObject cancelled=waitFor(cancelId);
            if(cancelled.optString("status").equals("cancelled"))check(!cancelled.getBoolean("card_changed")&&SsaCardImport.hash(card).equals(cancelHash),"Cancelled reset changed card");
            else check(cancelled.optString("status").equals("completed")&&cancelled.getBoolean("card_changed")&&SsaState.read(card)==SsaState.INIT_AUTH,"Cancellation misreported committed operation: "+cancelled);
            result.putString("stream","PASS McpSecurityCardInstrumentedTest "+checks+" checks: authenticated endpoint, exact-card/consent/manual/active-session guards, full backup, 714-byte reset, INIT_AUTH/hash readback, durable once-only request; no USB, firmware keys or vehicle commands");
        }catch(Throwable e){code=0;StringWriter trace=new StringWriter();e.printStackTrace(new PrintWriter(trace));result.putString("stream","FAIL: "+trace);}
        finally{if(a!=null){final Activity host=a;runOnMainSync(()->{AndroidMcp.unbind(host);host.finish();});}finish(code,result);}
    }
}
