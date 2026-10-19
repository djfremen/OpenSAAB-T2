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

/** Foreground, token-authenticated app-owned IPC. ADB forwarding carries no authority itself. */
public final class AndroidMcp {
    public interface Owner {
        JSONObject state()throws Exception;
        default JSONObject inspect()throws Exception{return state();}
        default void close(){}
        boolean key(int code);void startOffline();void stop();
    }
    private static Owner owner;
    private static Activity activity;
    private static android.content.Context application;
    private static volatile LocalServerSocket server;
    private static Thread listener;
    private static volatile String token="";
    private static ThreadPoolExecutor clients;
    private static final java.util.Set<LocalSocket> sockets=ConcurrentHashMap.newKeySet();
    private static final Semaphore waiter=new Semaphore(1);
    private static final Object signal=new Object();
    private static long changeSequence;
    private static long uiHeartbeat,manualGeneration;
    private static String admittedSession="";private static long admittedSequence=-1;
    private static final Handler ui=new Handler(Looper.getMainLooper());
    private static final Runnable beat=new Runnable(){public void run(){uiHeartbeat=SystemClock.elapsedRealtime();if(server!=null)ui.postDelayed(this,1000);}};
    public static synchronized void bind(Activity a,Owner o){
        if(owner!=null&&owner!=o)owner.close();activity=a;owner=o;application=a.getApplicationContext();
        if(application.getSharedPreferences("mcp_control",0).getBoolean("enabled",true))try{enable(a);}catch(IOException e){android.util.Log.w("OpenSaabMcp","Local MCP endpoint could not start",e);}
        changed();
    }
    public static synchronized void unbind(Activity a){if(activity==a){closeEndpoint(false);owner=null;activity=null;}}
    public static void manual(){manualGeneration++;changed();}
    public static void changed(){synchronized(signal){changeSequence++;signal.notifyAll();}}
    public static synchronized boolean enabled(){return server!=null;}
    public static void show(Activity a){
        if(enabled())new AlertDialog.Builder(a).setTitle("AI control (MCP)").setMessage("Enabled while OpenSAAB is in the foreground. Pairing code:\n"+token+"\n\nPair the local Android MCP bridge over this device’s paired ADB connection. The code is retained across app restarts. Manual keypad input cancels automatic navigation. Disable stays off until you enable it again.")
            .setPositiveButton("Done",null).setNegativeButton("Disable",(d,w)->disable()).show();
        else new AlertDialog.Builder(a).setTitle("AI control (MCP)").setMessage("Enable local AI inspection and firmware keys while OpenSAAB is in the foreground. Requests require a pairing code, current session and menu revision. Physical ignition and security dialogs remain operator actions.")
            .setPositiveButton("Enable",(d,w)->{try{enable(a);show(a);}catch(Exception e){android.widget.Toast.makeText(a,"MCP endpoint unavailable",android.widget.Toast.LENGTH_LONG).show();}}).setNegativeButton("Cancel",null).show();
    }
    private static File tokenFile(){return new File(application.getNoBackupFilesDir(),"mcp-pairing-token");}
    private static String pairingCode()throws IOException {
        File file=tokenFile();
        if(file.isFile())try(FileInputStream input=new FileInputStream(file)){
            byte[] bytes=new byte[65];int length=0,n;
            while(length<bytes.length&&(n=input.read(bytes,length,bytes.length-length))!=-1)length+=n;
            String saved=new String(bytes,0,length,StandardCharsets.US_ASCII);
            if(saved.matches("[0-9a-f]{64}"))return saved;
        }
        byte[] random=new byte[32];new SecureRandom().nextBytes(random);StringBuilder b=new StringBuilder();for(byte v:random)b.append(String.format(java.util.Locale.ROOT,"%02x",v&255));
        File temp=new File(application.getNoBackupFilesDir(),"mcp-pairing-token.tmp");
        try(FileOutputStream out=new FileOutputStream(temp)){
            temp.setReadable(false,false);temp.setWritable(false,false);temp.setReadable(true,true);temp.setWritable(true,true);
            out.write(b.toString().getBytes(StandardCharsets.US_ASCII));out.getFD().sync();
        }
        if(!temp.renameTo(file)){temp.delete();throw new IOException("Pairing credential could not be stored");}
        return b.toString();
    }
    public static synchronized void enable(Activity a)throws IOException {
        application=a.getApplicationContext();
        if(!application.getSharedPreferences("mcp_control",0).getBoolean("enabled",true))application.getSharedPreferences("mcp_control",0).edit().putBoolean("enabled",true).commit();
        if(server!=null)return;
        // A private, unbacked credential keeps an already paired local bridge usable after resume/restart.
        String code=pairingCode();LocalServerSocket endpoint=new LocalServerSocket(a.getPackageName()+".mcp");token=code;server=endpoint;admittedSession="";admittedSequence=-1;ui.post(beat);
        ThreadPoolExecutor workers=new ThreadPoolExecutor(0,4,15,TimeUnit.SECONDS,new SynchronousQueue<>(),r->new Thread(r,"android-mcp-client"));clients=workers;
        listener=new Thread(()->serve(endpoint,workers),"android-mcp-endpoint");listener.start();changed();
    }
    public static synchronized void disable(){if(application!=null)application.getSharedPreferences("mcp_control",0).edit().putBoolean("enabled",false).commit();closeEndpoint(true);}
    private static void closeEndpoint(boolean revokePairing){
        LocalServerSocket s=server;Thread previous=listener;server=null;listener=null;token="";ui.removeCallbacks(beat);if(owner!=null)owner.close();changed();
        if(revokePairing&&application!=null)tokenFile().delete();
        if(s!=null){
            // Closing an fd alone can leave blocking accept holding the abstract
            // address. Shutdown wakes accept so immediate foreground rebind works.
            try{android.system.Os.shutdown(s.getFileDescriptor(),android.system.OsConstants.SHUT_RDWR);}catch(android.system.ErrnoException ignored){}
            try{s.close();}catch(IOException ignored){}
        }
        for(LocalSocket socket:sockets)try{socket.close();}catch(IOException ignored){}
        if(clients!=null){clients.shutdownNow();clients=null;}
        if(previous!=null&&previous!=Thread.currentThread())try{previous.join(250);}catch(InterruptedException interrupted){Thread.currentThread().interrupt();}
    }
    private static byte[] read(InputStream source)throws IOException{InputStream in=new BufferedInputStream(source,4096);ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=in.read())!=-1&&n!=10){if(b.size()>=16384)throw new IOException("Request exceeds limit");b.write(n);}if(n<0)throw new EOFException();return b.toByteArray();}
    private static void serve(LocalServerSocket endpoint,ThreadPoolExecutor workers){
        while(server==endpoint){try{
            LocalSocket socket=endpoint.accept();sockets.add(socket);
            try{workers.execute(()->handle(endpoint,socket));}catch(RejectedExecutionException full){sockets.remove(socket);socket.close();}
        }catch(IOException closed){if(server!=endpoint)return;}}
    }
    private static void handle(LocalServerSocket endpoint,LocalSocket socket){
        try(LocalSocket connection=socket){
            connection.setSoTimeout(3000);JSONObject request=new JSONObject(new String(read(connection.getInputStream()),StandardCharsets.UTF_8));
            String supplied=request.optString("token"),expected=token;
            if(server!=endpoint||expected.isEmpty()||!MessageDigest.isEqual(expected.getBytes(StandardCharsets.US_ASCII),supplied.getBytes(StandardCharsets.US_ASCII)))throw new IOException("Authentication rejected");
            JSONObject response;
            try{response=request.optString("op").equals("wait_state")?waitState(endpoint,request):onUi(endpoint,request);}
            catch(Exception e){response=new JSONObject().put("error",e.getMessage()==null?"Request failed":e.getMessage());}
            connection.getOutputStream().write((response.toString()+"\n").getBytes(StandardCharsets.UTF_8));
        }catch(Exception ignored){}finally{sockets.remove(socket);}
    }
    private static JSONObject onUi(LocalServerSocket endpoint,JSONObject request)throws Exception {
        return onUi(endpoint,request,SystemClock.elapsedRealtime()+2000);
    }
    private static JSONObject onUi(LocalServerSocket endpoint,JSONObject request,long deadline)throws Exception {
        FutureTask<JSONObject> task=new FutureTask<>(()->{if(SystemClock.elapsedRealtime()>deadline||server!=endpoint)throw new IOException("Request expired");return dispatch(request);});ui.post(task);
        try{return task.get(Math.max(1,deadline-SystemClock.elapsedRealtime()+100),TimeUnit.MILLISECONDS);}
        catch(ExecutionException e){throw new IOException(e.getCause().getMessage(),e.getCause());}
        catch(Exception e){task.cancel(false);throw new IOException("Request expired; current UI state unavailable",e);}
    }
    private static JSONObject waitState(LocalServerSocket endpoint,JSONObject request)throws Exception {
        int duration=request.optInt("wait_ms",1000);
        if(duration<1||duration>1500||!request.has("after_sequence")||!request.has("after_revision"))throw new IOException("Bounded transition wait required");
        if(!waiter.tryAcquire())throw new IOException("Another transition wait is active");
        long began=SystemClock.elapsedRealtime(),deadline=began+duration;
        try{
            JSONObject last=null;
            while(server==endpoint){
                if(last!=null&&SystemClock.elapsedRealtime()>=deadline)return new JSONObject().put("state",last).put("observed_transition",false).put("wait_elapsed_ms",SystemClock.elapsedRealtime()-began);
                long observed;synchronized(signal){observed=changeSequence;}
                JSONObject state=onUi(endpoint,new JSONObject().put("op","state"),deadline);
                last=state;
                if(!request.optString("session").equals(state.optString("session"))||!state.optBoolean("session_running")||request.optLong("manual_generation",-1)!=state.optLong("manual_generation",-2))throw new IOException("Session replacement or manual takeover");
                JSONObject menu=state.optJSONObject("menu");
                boolean transitioned=menu!=null&&state.optBoolean("input_idle")&&menu.optBoolean("input_idle")&&menu.optLong("input_sequence",-1)>request.getLong("after_sequence")&&!menu.optString("revision").equals(request.getString("after_revision"));
                long remaining=deadline-SystemClock.elapsedRealtime();
                if(transitioned||remaining<=0)return new JSONObject().put("state",state).put("observed_transition",transitioned).put("wait_elapsed_ms",SystemClock.elapsedRealtime()-began);
                // Observer notifications wake this worker immediately. The bounded
                // fallback also covers owners without file observers and liveness changes.
                synchronized(signal){if(observed==changeSequence&&server==endpoint)signal.wait(Math.min(remaining,100));}
            }
            throw new IOException("Endpoint revoked");
        }finally{waiter.release();}
    }
    private static JSONObject dispatch(JSONObject request)throws Exception {
        if(owner==null||activity==null||activity.isFinishing())throw new IOException("Foreground owner unavailable");
        String op=request.optString("op");JSONObject state=(op.equals("state")||op.equals("health")||op.equals("crash_details"))?owner.inspect():owner.state();state.put("enabled",true).put("app_version",AppBuildProfile.installedVersion(activity)).put("architecture",AppBuildProfile.abi(activity)).put("app_pid",android.os.Process.myPid()).put("manual_generation",manualGeneration).put("mcp_features",new JSONArray().put("wait_state_v1"));
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
