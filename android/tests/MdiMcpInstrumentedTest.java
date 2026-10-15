// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.*;
import android.os.*;
import java.io.*;
import java.lang.reflect.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.UUID;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import org.json.*;

/** Actual installed MDI activity/worker and shared MCP, offline carrier fixture. */
public final class MdiMcpInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    private Field field(Activity a,String name)throws Exception{Field f=a.getClass().getDeclaredField(name);f.setAccessible(true);return f;}
    private JSONObject call(String token,String op,JSONObject args)throws Exception{
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(4000);
            args.put("token",token).put("op",op);socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));
            ByteArrayOutputStream out=new ByteArrayOutputStream();int n;
            while((n=socket.getInputStream().read())!=-1&&n!=10){if(out.size()>131072)throw new IOException("Response limit");out.write(n);}
            return new JSONObject(out.toString("UTF-8"));
        }
    }
    private JSONObject waitMenu(String token,String text)throws Exception{
        long deadline=SystemClock.elapsedRealtime()+30000;JSONObject state=null;
        while(SystemClock.elapsedRealtime()<deadline){
            state=call(token,"state",new JSONObject());JSONObject menu=state.optJSONObject("menu");
            if(menu!=null&&menu.optString("text").contains(text)&&menu.optBoolean("input_idle")&&state.optBoolean("input_idle")&&state.optBoolean("emulator_alive"))return state;
            Thread.sleep(50);
        }
        throw new AssertionError("MDI worker did not reach "+text+": "+state);
    }
    private JSONObject request(JSONObject state,int code)throws Exception{return new JSONObject().put("session",state.getString("session")).put("revision",state.getJSONObject("menu").getString("revision")).put("manual_generation",state.getLong("manual_generation")).put("encoder",code);}
    public void onStart(){Bundle result=new Bundle();Activity activity=null;java.lang.Process worker=null;InteractiveKeyPump pump=null;AtomicBoolean running=null;File output=null;int code=0;
        try{
            Context context=getTargetContext();check(SimulatorVehicleConnection.available(context),"Disposable Android emulator required");
            check(AppBuildProfile.installedVersion(context).equals("0.1.0-preview.90"),"Wrong installed attempt");
            FirmwareStore store=new FirmwareStore(context.getFilesDir());check(store.missing().isEmpty(),"Real firmware required");
            activity=startActivitySync(new Intent(context,MdiUsbActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();final Activity a=activity;
            File run=new File(context.getFilesDir(),"mdi-sessions/"+UUID.randomUUID());output=new File(run,"native");Files.createDirectories(output.toPath());
            File directory=output;
            pump=new InteractiveKeyPump(new File(output,"native-key.txt"),e->{throw new AssertionError(e);});
            field(a,"directory").set(a,run);field(a,"nativeDirectory").set(a,output);field(a,"keyPump").set(a,pump);
            running=(AtomicBoolean)field(a,"running").get(a);running.set(true);
            ProcessBuilder builder=new ProcessBuilder(new File(context.getApplicationInfo().nativeLibraryDir,"libtech2_mdi.so").getAbsolutePath(),
                "--test-harness","--harness-target","native-manual","--headless","--boot",new File(store.firmware,"eprom.bin").getAbsolutePath(),
                "--opsys",new File(store.firmware,"opsys.dwn").getAbsolutePath(),"--max-insns","50000000000","--output-dir",output.getAbsolutePath(),new File(store.firmware,"card.bin").getAbsolutePath());
            builder.environment().put("TECH2_ANDROID_LIVE","1");builder.redirectErrorStream(true).redirectOutput(new File(run,"offline-mdi-mcp-worker.log"));worker=builder.start();
            runOnMainSync(()->{try{AndroidMcp.enable(a);}catch(Exception e){throw new RuntimeException(e);}});
            String token=new String(Files.readAllBytes(new File(context.getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            // NativeManual's existing harness reaches Main Menu before handing off keys.
            JSONObject state=waitMenu(token,"Main Menu");
            check(state.getString("mode").equals("mdi")&&state.getString("session").equals(run.getName()),"MDI owner/session not bound to unique run");
            check(state.getJSONObject("menu").getString("source").equals("original_guest_lcd"),"MDI fabricated menu source");
            check(state.getJSONObject("menu").getInt("process_id")>0,"Actual MDI process identity absent");
            JSONObject stale=request(state,24).put("revision","wrong");check(call(token,"key",stale).has("error"),"MDI stale revision admitted");
            check(call(token,"key",request(state,24)).getBoolean("accepted"),"MDI F0 refused");state=waitMenu(token,"Model Year");
            JSONObject beforeManual=state;
            runOnMainSync(()->{android.view.View exit=a.findViewById(android.R.id.content).findViewWithTag("tech2-key-1");check(exit!=null,"MDI manual EXIT control absent");exit.performClick();});
            check(call(token,"key",request(beforeManual,12)).has("error"),"MDI manual takeover ignored");state=waitMenu(token,"Main Menu");
            check(call(token,"stop",new JSONObject().put("session",run.getName())).getBoolean("accepted"),"MDI Stop refused");
            long until=SystemClock.elapsedRealtime()+3000;while(!new File(run,"session-stop").isFile()&&SystemClock.elapsedRealtime()<until)Thread.sleep(10);
            check(new File(run,"session-stop").isFile(),"MCP Stop did not use MDI carrier stop contract");
            // The fixture replaces only the absent USB carrier's stop forwarding.
            Files.write(new File(output,"native-stop").toPath(),"stop\n".getBytes(StandardCharsets.US_ASCII));
            check(worker.waitFor(5,TimeUnit.SECONDS),"MDI worker failed clean Stop");running.set(false);
            JSONObject after=call(token,"state",new JSONObject());check(!after.getBoolean("session_running")&&!after.getBoolean("emulator_alive"),"MDI current health retained a dead worker");
            JSONObject receipt=new JSONObject().put("result","pass").put("version",AppBuildProfile.installedVersion(context)).put("scope","actual installed MDI activity, worker and MCP; offline USB carrier fixture, no physical adapter")
                .put("unique_session",run.getName()).put("native_menu",state.getJSONObject("menu")).put("stale_rejection",true).put("manual_takeover",true).put("carrier_stop_requested",true).put("worker_forced_stop",false);
            FirmwareStore.writeJson(new File(context.getCacheDir(),"mdi-mcp-receipt.json"),receipt);
            result.putString("stream","PASS: installed MDI activity/shared MCP and packaged MDI worker: original guest Main Menu/F0/Model Year/manual EXIT, unique session, actual process liveness, stale/manual rejection and carrier stop contract; offline fixture, no USB or vehicle commands\n");code=-1;
        }catch(Throwable error){result.putString("stream","FAIL: "+error+"\n");}
        finally{if(output!=null)try{Files.write(new File(output,"native-stop").toPath(),"stop\n".getBytes(StandardCharsets.US_ASCII));}catch(Exception ignored){}if(worker!=null&&worker.isAlive())try{if(!worker.waitFor(5,TimeUnit.SECONDS))worker.destroyForcibly();}catch(Exception ignored){}if(running!=null)running.set(false);if(pump!=null)pump.close();final Activity a=activity;runOnMainSync(()->{AndroidMcp.disable();if(a!=null)a.finish();});}
        finish(code,result);
    }
}
