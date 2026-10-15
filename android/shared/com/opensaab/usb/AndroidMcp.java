// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.net.*;
import android.os.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.security.*;
import java.util.concurrent.*;
import org.json.*;

/** Explicit opt-in, token-authenticated app-owned IPC. ADB forwarding carries no authority itself. */
public final class AndroidMcp {
    public interface Owner {JSONObject state()throws Exception;boolean key(int code);void startOffline();void stop();}
    private static Owner owner;
    private static Activity activity;
    private static LocalServerSocket server;
    private static String token="";
    private static long uiHeartbeat,manualGeneration;
    private static String admittedSession="";private static long admittedSequence=-1;
    private static final Handler ui=new Handler(Looper.getMainLooper());
    private static final Runnable beat=new Runnable(){public void run(){uiHeartbeat=SystemClock.elapsedRealtime();if(server!=null)ui.postDelayed(this,1000);}};
    public static synchronized void bind(Activity a,Owner o){activity=a;owner=o;}
    public static synchronized void unbind(Activity a){if(activity==a){disable();owner=null;activity=null;}}
    public static void manual(){manualGeneration++;}
    public static synchronized boolean enabled(){return server!=null;}
    public static void show(Activity a){
        if(enabled())new AlertDialog.Builder(a).setTitle("AI control (MCP)").setMessage("Enabled for this app session. Pairing code:\n"+token+"\n\nUse the local Android MCP bridge over this phone’s paired ADB connection. Manual keypad input cancels automatic navigation.")
            .setPositiveButton("Done",null).setNegativeButton("Disable",(d,w)->disable()).show();
        else new AlertDialog.Builder(a).setTitle("AI control (MCP)").setMessage("Enable local AI inspection and firmware keys for this foreground session. Requests require a pairing code, current session and menu revision. Physical ignition and security dialogs remain operator actions.")
            .setPositiveButton("Enable",(d,w)->{try{enable(a);show(a);}catch(Exception e){android.widget.Toast.makeText(a,"MCP endpoint unavailable",android.widget.Toast.LENGTH_LONG).show();}}).setNegativeButton("Cancel",null).show();
    }
    public static synchronized void enable(Activity a)throws Exception {
        if(server!=null)return;
        byte[] random=new byte[32];new SecureRandom().nextBytes(random);StringBuilder b=new StringBuilder();for(byte v:random)b.append(String.format(java.util.Locale.ROOT,"%02x",v&255));token=b.toString();
        // Preview tooling may read this private file with run-as. Production pairing uses the displayed code.
        File file=new File(a.getNoBackupFilesDir(),"mcp-pairing-token");try(FileOutputStream out=new FileOutputStream(file)){out.write(token.getBytes(StandardCharsets.US_ASCII));}file.setReadable(false,false);file.setWritable(false,false);file.setReadable(true,true);file.setWritable(true,true);
        LocalServerSocket endpoint=new LocalServerSocket(a.getPackageName()+".mcp");server=endpoint;admittedSession="";admittedSequence=-1;ui.post(beat);
        new Thread(()->serve(endpoint),"android-mcp-endpoint").start();
    }
    public static synchronized void disable(){LocalServerSocket s=server;server=null;token="";ui.removeCallbacks(beat);if(activity!=null)new File(activity.getNoBackupFilesDir(),"mcp-pairing-token").delete();if(s!=null)try{s.close();}catch(IOException ignored){}}
    private static byte[] read(InputStream in)throws IOException{ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=in.read())!=-1&&n!=10){if(b.size()>=16384)throw new IOException("Request exceeds limit");b.write(n);}if(n<0)throw new EOFException();return b.toByteArray();}
    private static void serve(LocalServerSocket endpoint){
        while(server==endpoint){try(LocalSocket socket=endpoint.accept()){
            socket.setSoTimeout(3000);JSONObject request=new JSONObject(new String(read(socket.getInputStream()),StandardCharsets.UTF_8));
            String supplied=request.optString("token");if(token.isEmpty()||!MessageDigest.isEqual(token.getBytes(StandardCharsets.US_ASCII),supplied.getBytes(StandardCharsets.US_ASCII)))throw new IOException("Authentication rejected");
            final long deadline=SystemClock.elapsedRealtime()+2000;FutureTask<JSONObject> task=new FutureTask<>(()->{if(SystemClock.elapsedRealtime()>deadline||server!=endpoint)throw new IOException("Request expired");return dispatch(request);});ui.post(task);
            JSONObject response;try{response=task.get(2100,TimeUnit.MILLISECONDS);}catch(Exception e){task.cancel(false);response=new JSONObject().put("error",e.getCause()!=null?e.getCause().getMessage():"Request expired; current UI state unavailable");}
            socket.getOutputStream().write((response.toString()+"\n").getBytes(StandardCharsets.UTF_8));
        }catch(Exception ignored){}}
    }
    private static JSONObject dispatch(JSONObject request)throws Exception {
        if(owner==null||activity==null||activity.isFinishing())throw new IOException("Foreground owner unavailable");
        String op=request.optString("op");JSONObject state=owner.state();state.put("enabled",true).put("app_version",AppBuildProfile.installedVersion(activity)).put("architecture",AppBuildProfile.abi(activity)).put("app_pid",android.os.Process.myPid()).put("manual_generation",manualGeneration);
        if(op.equals("state"))return state;
        if(op.equals("health")||op.equals("crash_details")){
            long now=SystemClock.elapsedRealtime();JSONObject facts=new JSONObject().put("app_alive",true).put("ui_heartbeat_age_ms",now-uiHeartbeat).put("session_running",state.optBoolean("session_running")).put("emulator_alive",state.optBoolean("emulator_alive")).put("emulator_heartbeat_age_ms",state.opt("emulator_heartbeat_age_ms")).put("session_age_ms",state.opt("session_age_ms"));
            long guestAge=state.optLong("emulator_heartbeat_age_ms",-1),sessionAge=state.optLong("session_age_ms",0);
            String status=now-uiHeartbeat>=20000?"ui_unresponsive_suspected":state.optBoolean("session_running")&&!state.optBoolean("emulator_alive")?"emulator_exited":state.optBoolean("session_running")&&(guestAge>=30000||(guestAge<0&&sessionAge>=180000))?"emulator_unresponsive_suspected":"responsive";
            JSONObject result=new JSONObject().put("schema",1).put("app_version",AppBuildProfile.installedVersion(activity)).put("current",new JSONObject().put("state",status).put("facts",facts).put("static_screen_is_failure",false)).put("history_is_current_failure",false);
            if(op.equals("crash_details")&&Build.VERSION.SDK_INT>=30){JSONArray exits=new JSONArray();ActivityManager am=(ActivityManager)activity.getSystemService(Activity.ACTIVITY_SERVICE);for(ApplicationExitInfo exit:am.getHistoricalProcessExitReasons(activity.getPackageName(),0,8))exits.put(new JSONObject().put("timestamp_ms",exit.getTimestamp()).put("reason",exit.getReason()).put("status",exit.getStatus()));result.put("historical_OS_exits",exits);}
            return result;
        }
        if(op.equals("start_offline")){if(state.optBoolean("busy")||state.optBoolean("session_running"))throw new IOException("Session busy");owner.startOffline();return new JSONObject().put("accepted",true).put("completion_verified",false);}
        if(!request.optString("session").equals(state.optString("session"))||state.optString("session").isEmpty()||!state.optBoolean("session_running"))throw new IOException("Session changed");
        if(op.equals("stop")){owner.stop();return new JSONObject().put("accepted",true).put("cleanup_verified",false);}
        JSONObject menu=state.optJSONObject("menu");
        if(menu==null||!request.optString("revision").equals(menu.optString("revision"))||request.optLong("manual_generation",-1)!=manualGeneration||!menu.optBoolean("input_idle")||!state.optBoolean("input_idle"))throw new IOException("Stale menu or manual takeover");
        if(!op.equals("key"))throw new IOException("Unknown operation");
        int code=request.optInt("encoder",-1);boolean known=false;for(int k:new int[]{10,6,7,8,1,16,9,12,11,13,25,14,20,24,4,19,23,3,18,22,2,17,21})if(code==k)known=true;
        if(!known)throw new IOException("Unknown firmware key");
        if(state.optString("session").equals(admittedSession)&&menu.optLong("input_sequence",-1)<=admittedSequence)throw new IOException("Await the previous key consumption before sending another");
        boolean accepted=owner.key(code);if(accepted){admittedSession=state.optString("session");admittedSequence=menu.optLong("input_sequence",-1);}return new JSONObject().put("accepted",accepted).put("completion_verified",false).put("session",state.optString("session"));
    }
    private AndroidMcp(){}
}
