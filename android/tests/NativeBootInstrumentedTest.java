// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.*;
import android.os.*;
import java.io.*;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import org.json.*;

/** Exact installed Android offline boot/navigation; external real firmware, no adapter. */
public final class NativeBootInstrumentedTest extends Instrumentation {
    private final JSONArray stages=new JSONArray();
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    private JSONObject call(String token,String op,JSONObject args)throws Exception {
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(4000);
            args.put("token",token).put("op",op);socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));
            ByteArrayOutputStream out=new ByteArrayOutputStream();int n;
            while((n=socket.getInputStream().read())!=-1&&n!=10){if(out.size()>131072)throw new IOException("Response limit");out.write(n);}
            return new JSONObject(out.toString("UTF-8"));
        }
    }
    private JSONObject waitMenu(String token,String text)throws Exception {
        long deadline=SystemClock.elapsedRealtime()+60000;JSONObject state=null;
        while(SystemClock.elapsedRealtime()<deadline){
            state=call(token,"state",new JSONObject());JSONObject menu=state.optJSONObject("menu");
            if(menu!=null&&menu.optString("text").contains(text)&&state.optBoolean("input_idle")&&menu.optBoolean("input_idle")){
                check(menu.optString("source").equals("original_guest_lcd"),"Menu source must be original firmware");
                stages.put(new JSONObject().put("required_text",text).put("state",state));return state;
            }
            if(state!=null&&!state.optBoolean("session_running"))throw new AssertionError("Session stopped before "+text);
            Thread.sleep(50);
        }
        throw new AssertionError("Original firmware did not reach "+text+": "+state);
    }
    private JSONObject key(String token,int encoder,JSONObject state,String screen)throws Exception {
        JSONObject reply=call(token,"key",new JSONObject().put("session",state.getString("session"))
            .put("revision",state.getJSONObject("menu").getString("revision"))
            .put("manual_generation",state.getLong("manual_generation")).put("encoder",encoder));
        check(reply.optBoolean("accepted"),"Canonical key refused: "+encoder);
        return waitMenu(token,screen);
    }
    public void onStart(){Bundle result=new Bundle();Activity activity=null;String token="";int code=0;
        try{
            Context context=getTargetContext();check(SimulatorVehicleConnection.available(context),"Disposable Android emulator required");
            String version=context.getPackageManager().getPackageInfo(context.getPackageName(),0).versionName;
            check(version.equals("0.1.0-preview.85"),"Wrong installed attempt: "+version);
            String label=context.getApplicationInfo().loadLabel(context.getPackageManager()).toString();
            check(label.contains(version)&&label.contains("ARM64"),"Packaged app identity missing version/architecture");
            FirmwareStore store=new FirmwareStore(context.getFilesDir());check(store.missing().isEmpty(),"Real firmware required");
            JSONObject hashes=new JSONObject();for(String name:new String[]{"eprom.bin","opsys.dwn","candi.bin","card.bin"})hashes.put(name,OfflineSessionEvidence.digest(new File(store.firmware,name)));
            Intent intent=new Intent(Intent.ACTION_MAIN).setClassName(context,"com.opensaab.tech2.MainActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("mcp_test",true);
            activity=startActivitySync(intent);waitForIdleSync();final Activity owner=activity;
            runOnMainSync(()->{android.view.View proceed=owner.findViewById(android.R.id.content).findViewWithTag("preview-continue");check(proceed!=null,"Startup notice missing");proceed.performClick();});waitForIdleSync();
            runOnMainSync(()->{try{AndroidMcp.enable(owner);}catch(Exception error){throw new RuntimeException(error);}});
            Method start=activity.getClass().getDeclaredMethod("startSession");start.setAccessible(true);
            runOnMainSync(()->{try{start.invoke(owner);}catch(Exception error){throw new RuntimeException(error);}});
            token=new String(Files.readAllBytes(new File(context.getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            java.util.regex.Matcher firmwareVersion=java.util.regex.Pattern.compile("\\d+\\.\\d{3}").matcher(store.active().optString("label"));
            check(firmwareVersion.find(),"Selected firmware version required for the real boot fixture");
            // The firmware briefly draws version 0.000 while populating the
            // welcome. Submit ENTER only after the selected version is visible.
            JSONObject state=waitMenu(token,firmwareVersion.group());
            check(state.getJSONObject("connection").getString("state").equals("offline"),"Offline app opened a live transport");
            state=key(token,16,state,"Main Menu");state=key(token,24,state,"Model Year(s)");
            state=key(token,12,state,"2 / ");state=key(token,9,state,"1 / ");state=key(token,1,state,"Main Menu");
            String session=state.getString("session");
            check(call(token,"stop",new JSONObject().put("session",session)).getBoolean("accepted"),"Stop refused");
            long deadline=SystemClock.elapsedRealtime()+10000;
            do{Thread.sleep(50);state=call(token,"state",new JSONObject());}while(state.optBoolean("session_running")&&SystemClock.elapsedRealtime()<deadline);
            check(!state.optBoolean("session_running")&&!state.optBoolean("emulator_alive"),"Native process did not stop");
            File run=new File(context.getFilesDir(),"sessions/"+session);
            JSONObject report=FirmwareStore.json(new File(run,"report.json")),kernel=report.getJSONObject("psos");
            check(report.getString("mode").equals("fidelity")&&report.getString("opsys_image").isEmpty(),"Offline app used the research/support ROM path");
            check(kernel.getBoolean("active")&&kernel.isNull("error")&&kernel.getLong("switches")>0,"Recovered host kernel did not execute");
            check(report.getJSONObject("program_identity").getString("version").equals(version),"App and core versions differ");
            check(report.isNull("candi"),"Offline native boot started CANdi");
            JSONObject end=FirmwareStore.json(new File(run,"session-end.json"));check(!end.getBoolean("forced_stop")&&end.getString("reason").equals("user_stop"),"Stop was not clean");
            JSONObject contextEvidence=OfflineSessionEvidence.context(FirmwareStore.json(new File(run,"emulation-context.json")));
            check(contextEvidence.getString("profile").equals("offline_native"),"Native startup profile lost in saved evidence");
            JSONObject display=FirmwareStore.json(new File(run,"host-display.json"));
            check(display.getInt("signal_version")==3&&display.getBoolean("available")&&display.getJSONObject("signal_counts").getInt("4360")>0,"Genuine Data Display handshake missing");
            for(String name:new String[]{"eprom.bin","opsys.dwn","candi.bin","card.bin"})check(hashes.getString(name).equals(OfflineSessionEvidence.digest(new File(store.firmware,name))),"Input firmware modified: "+name);
            JSONObject receipt=new JSONObject().put("result","pass").put("version",version).put("label",label)
                .put("scope","installed Android ARM64 emulator; original offline firmware, no physical adapter")
                .put("input_hashes_before_and_after",hashes).put("firmware_selection",store.active()).put("session",session).put("stages",stages)
                .put("report",report).put("display",display).put("session_end",end).put("emulation_context",contextEvidence);
            FirmwareStore.writeJson(new File(context.getCacheDir(),"native-boot-receipt.json"),receipt);
            result.putString("stream","PASS: exact installed Android "+version+" ARM64, native fidelity "+store.active().optString("label")+" welcome, ENTER/Main Menu, F0/Model Year, DOWN/UP/EXIT, real host kernel/Data Display handshake, unchanged inputs and clean Stop; offline AVD only\n");code=-1;
        }catch(Throwable error){result.putString("stream","FAIL: "+error+"\n");}
        finally{
            try{if(!token.isEmpty()){JSONObject current=call(token,"state",new JSONObject());if(current.optBoolean("session_running")){call(token,"stop",new JSONObject().put("session",current.getString("session")));long until=SystemClock.elapsedRealtime()+10000;while(call(token,"state",new JSONObject()).optBoolean("session_running")&&SystemClock.elapsedRealtime()<until)Thread.sleep(50);}}}catch(Exception ignored){}
            final Activity owner=activity;runOnMainSync(()->{AndroidMcp.disable();if(owner!=null)owner.finish();});
        }
        finish(code,result);
    }
}
