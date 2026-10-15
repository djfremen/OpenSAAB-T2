// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.net.*;import android.os.*;
import java.io.*;import java.lang.reflect.Method;import java.nio.charset.StandardCharsets;import java.nio.file.Files;import org.json.*;
/** Actual installed recovered firmware + native menu/key endpoint, offline on a disposable AVD. */
public final class McpFirmwareInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private void check(boolean b,String why){if(!b)throw new AssertionError(why);}
    private JSONObject call(String token,String op,JSONObject args)throws Exception{
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(4000);args.put("token",token).put("op",op);
            socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));ByteArrayOutputStream out=new ByteArrayOutputStream();int n;while((n=socket.getInputStream().read())!=-1&&n!=10){if(out.size()>131072)throw new IOException("Response limit");out.write(n);}return new JSONObject(out.toString("UTF-8"));
        }
    }
    private void invoke(Activity a,String name,String value)throws Exception{Method m=value==null?a.getClass().getDeclaredMethod(name):a.getClass().getDeclaredMethod(name,String.class);m.setAccessible(true);runOnMainSync(()->{try{if(value==null)m.invoke(a);else m.invoke(a,value);}catch(Exception e){throw new RuntimeException(e);}});}
    private JSONObject waitMenu(String token,String text)throws Exception{
        long limit=SystemClock.elapsedRealtime()+60000;JSONObject state=null;
        while(SystemClock.elapsedRealtime()<limit){state=call(token,"state",new JSONObject());JSONObject menu=state.optJSONObject("menu");if(menu!=null&&menu.optString("text").contains(text)&&state.optBoolean("input_idle")&&menu.optBoolean("input_idle"))return state;Thread.sleep(50);}
        throw new AssertionError("No observed original firmware menu: "+text+"; session_running="+(state!=null&&state.optBoolean("session_running")));
    }
    private JSONObject send(String token,int encoder,JSONObject state)throws Exception{return call(token,"key",new JSONObject().put("session",state.getString("session")).put("revision",state.getJSONObject("menu").getString("revision")).put("manual_generation",state.getLong("manual_generation")).put("encoder",encoder));}
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;
        try{
            check(SimulatorVehicleConnection.available(getTargetContext()),"Disposable debug emulator required");check(new FirmwareStore(getTargetContext().getFilesDir()).missing().isEmpty(),"Install original firmware in AVD first");
            Intent intent=new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("mcp_test",true);activity=startActivitySync(intent);waitForIdleSync();final Activity a=activity; runOnMainSync(()->{android.view.View notice=a.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(notice!=null)notice.performClick();});waitForIdleSync();
            runOnMainSync(()->{try{AndroidMcp.enable(a);}catch(Exception e){throw new RuntimeException(e);}});invoke(activity,"startSession",null);
            String token=new String(Files.readAllBytes(new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            java.util.regex.Matcher selectedVersion=java.util.regex.Pattern.compile("\\d+\\.\\d{3}").matcher(new FirmwareStore(getTargetContext().getFilesDir()).active().optString("label"));check(selectedVersion.find(),"Selected firmware version required");
            JSONObject state=waitMenu(token,selectedVersion.group()),splash=state;
            check(state.getJSONObject("menu").getString("source").equals("original_guest_lcd"),"MCP fabricated the menu source");check(send(token,16,state).getBoolean("accepted"),"Original Enter refused");
            state=waitMenu(token,"Main Menu");check(!state.getJSONObject("menu").getString("revision").equals(splash.getJSONObject("menu").getString("revision")),"Splash revision did not transition");
            JSONArray entries=state.getJSONObject("menu").getJSONArray("entries");int found=0;for(int i=0;i<entries.length();i++)if(entries.getJSONObject(i).optString("label").equals("Diagnostics")){check(entries.getJSONObject(i).getInt("encoder")==24,"Canonical Diagnostics key changed");found++;}check(found==1,"Exact Diagnostics label unavailable");
            check(send(token,24,state).getBoolean("accepted"),"Visible Diagnostics key refused");JSONObject year=waitMenu(token,"Model Year");
            check(send(token,12,state).has("error"),"Stale actual menu admitted");long generation=year.getLong("manual_generation");invoke(activity,"enqueue","0x0c");JSONObject current=call(token,"state",new JSONObject());check(current.getLong("manual_generation")>generation,"Manual keypad did not revoke navigation");
            check(send(token,12,year).has("error"),"Manual takeover admitted stale key");
            String session=current.getString("session");check(call(token,"stop",new JSONObject().put("session",session)).getBoolean("accepted"),"Session-bound Stop refused");
            long limit=SystemClock.elapsedRealtime()+10000;do{Thread.sleep(50);current=call(token,"state",new JSONObject());}while(current.optBoolean("session_running")&&SystemClock.elapsedRealtime()<limit);
            check(!current.optBoolean("session_running")&&!current.optBoolean("emulator_alive"),"Recovered process did not stop");JSONObject end=FirmwareStore.json(new File(getTargetContext().getFilesDir(),"sessions/"+session+"/session-end.json"));check(!end.getBoolean("forced_stop"),"Normal Stop forced the recovered process");
            result.putString("stream","PASS: actual recovered original firmware welcome, Enter, Main Menu, exact Diagnostics entry, Model Year, semantic revisions, stale rejection, manual takeover and clean session-bound MCP Stop; offline AVD, no vehicle commands\n");
        }catch(Throwable error){code=0;result.putString("stream","FAIL: "+error+"\n");}finally{final Activity a=activity;runOnMainSync(()->{AndroidMcp.disable();if(a!=null)a.finish();});}finish(code,result);
    }
}
