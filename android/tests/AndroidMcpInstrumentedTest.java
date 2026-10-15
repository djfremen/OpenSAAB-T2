// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.net.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;import org.json.*;
/** Actual endpoint authentication, revocation and stale-input tests; synthetic owner, no firmware or radio. */
public final class AndroidMcpInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    private static final class Owner implements AndroidMcp.Owner {
        String session="fixture-session";long sequence=0;boolean idle=true;int keys;
        public JSONObject state()throws Exception{return new JSONObject().put("session",session).put("session_running",true).put("emulator_alive",true).put("input_idle",idle).put("busy",false).put("session_age_ms",1).put("emulator_heartbeat_age_ms",1).put("menu",new JSONObject().put("revision","fixture-menu").put("input_idle",true).put("input_sequence",sequence));}
        public boolean key(int code){keys++;return true;}public void startOffline(){}public void stop(){}
    }
    private JSONObject call(String token,String op,JSONObject args)throws Exception{
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(4000);
            args.put("token",token).put("op",op);socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));
            ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=socket.getInputStream().read())!=-1&&n!=10)b.write(n);if(b.size()==0)throw new EOFException("Endpoint denied request");return new JSONObject(b.toString("UTF-8"));
        }
    }
    private JSONObject key(String session,String revision,long manual)throws Exception{return new JSONObject().put("session",session).put("revision",revision).put("manual_generation",manual).put("encoder",24);}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;
        try{
            check(SimulatorVehicleConnection.available(getTargetContext()),"Disposable debug emulator required");
            Intent intent=new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("mcp_test",true);activity=startActivitySync(intent);waitForIdleSync();Owner owner=new Owner();final Activity a=activity;
            runOnMainSync(()->{android.view.View notice=a.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(notice!=null)notice.performClick();});waitForIdleSync();
            runOnMainSync(()->{AndroidMcp.bind(a,owner);try{AndroidMcp.enable(a);}catch(Exception e){throw new RuntimeException(e);}});waitForIdleSync();
            String token=new String(Files.readAllBytes(new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            JSONObject state=call(token,"state",new JSONObject());long manual=state.getLong("manual_generation");check(state.getString("app_version").equals("0.1.0-preview.90"),"Installed version unavailable");
            boolean denied=false;try{call("0".repeat(64),"state",new JSONObject());}catch(EOFException expected){denied=true;}check(denied,"Bad token was accepted");
            check(call(token,"key",key("wrong","fixture-menu",manual)).has("error")&&owner.keys==0,"Stale session admitted");
            check(call(token,"key",key(owner.session,"wrong",manual)).has("error")&&owner.keys==0,"Stale revision admitted");
            check(call(token,"key",key(owner.session,"fixture-menu",manual)).getBoolean("accepted")&&owner.keys==1,"Canonical key rejected");
            check(call(token,"key",key(owner.session,"fixture-menu",manual)).has("error")&&owner.keys==1,"Duplicate key before consumption admitted");
            runOnMainSync(()->owner.sequence++);check(call(token,"key",key(owner.session,"fixture-menu",manual)).getBoolean("accepted")&&owner.keys==2,"New consumed sequence rejected");
            runOnMainSync(AndroidMcp::manual);check(call(token,"key",key(owner.session,"fixture-menu",manual)).has("error")&&owner.keys==2,"Manual takeover ignored");
            runOnMainSync(()->owner.idle=false);state=call(token,"state",new JSONObject());check(call(token,"key",key(owner.session,"fixture-menu",state.getLong("manual_generation"))).has("error"),"Busy input admitted");
            JSONObject health=call(token,"health",new JSONObject());check(!health.getJSONObject("current").getBoolean("static_screen_is_failure"),"Static LCD became failure");check(!call(token,"crash_details",new JSONObject()).getBoolean("history_is_current_failure"),"OS history became current failure");
            runOnMainSync(AndroidMcp::disable);check(!new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").exists(),"Pairing token survived revocation");
            denied=false;try{call(token,"state",new JSONObject());}catch(IOException expected){denied=true;}check(denied,"Disabled endpoint accepted request");
            result.putString("stream","PASS: installed Android MCP endpoint: authentication, metadata, stale session/menu, consumed sequence, duplicate admission, manual takeover, busy input, current health/history separation, revocation and private token deletion; synthetic owner, no firmware/vehicle commands\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}finally{final Activity a=activity;runOnMainSync(()->{AndroidMcp.disable();if(a!=null){AndroidMcp.unbind(a);a.finish();}});}finish(code,result);
    }
}
