// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.net.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.*;import java.util.concurrent.*;import org.json.*;
/** Actual endpoint authentication, revocation and stale-input tests; synthetic owner, no firmware or radio. */
public final class AndroidMcpInstrumentedTest extends Instrumentation {
    private Bundle arguments;
    public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
    private static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    private static final class Owner implements AndroidMcp.Owner {
        String session="fixture-session",revision="fixture-menu";long sequence=0;boolean idle=true,running=true;int keys,stops;
        public JSONObject state()throws Exception{return new JSONObject().put("session",session).put("session_running",running).put("emulator_alive",running).put("input_idle",idle).put("busy",false).put("session_age_ms",1).put("emulator_heartbeat_age_ms",1).put("menu",new JSONObject().put("revision",revision).put("input_idle",true).put("input_sequence",sequence));}
        public boolean key(int code){keys++;return true;}public void startOffline(){}public void stop(){stops++;running=false;AndroidMcp.changed();}
    }
    private JSONObject call(String token,String op,JSONObject args)throws Exception{
        try(LocalSocket socket=new LocalSocket()){
            socket.connect(new LocalSocketAddress("com.opensaab.tech2.mcp"));socket.setSoTimeout(4000);
            args.put("token",token).put("op",op);socket.getOutputStream().write((args.toString()+"\n").getBytes(StandardCharsets.UTF_8));
            ByteArrayOutputStream b=new ByteArrayOutputStream();int n;while((n=socket.getInputStream().read())!=-1&&n!=10)b.write(n);if(b.size()==0)throw new EOFException("Endpoint denied request");return new JSONObject(b.toString("UTF-8"));
        }
    }
    private JSONObject key(String session,String revision,long manual)throws Exception{return new JSONObject().put("session",session).put("revision",revision).put("manual_generation",manual).put("encoder",24);}
    private JSONObject waitArgs(JSONObject state,int milliseconds)throws Exception{return new JSONObject().put("session",state.getString("session")).put("manual_generation",state.getLong("manual_generation")).put("after_sequence",state.getJSONObject("menu").getLong("input_sequence")).put("after_revision",state.getJSONObject("menu").getString("revision")).put("wait_ms",milliseconds);}
    private void cacheTests()throws Exception {
        File root=new File(getTargetContext().getCacheDir(),"mcp-cache-fixture");root.mkdirs();File menu=new File(root,"native-menu.json"),temp=new File(root,"native-menu.json.tmp");
        McpMenuCache cache=new McpMenuCache();
        try{
            Files.write(menu.toPath(),"{\"revision\":\"first\"}".getBytes(StandardCharsets.UTF_8));
            JSONObject first=cache.get(root,false);first.put("revision","mutated client copy");check(cache.get(root,false).getString("revision").equals("first"),"Mutable snapshot escaped cache");
            Files.write(temp.toPath(),"{\"revision\":\"second\"}".getBytes(StandardCharsets.UTF_8));Files.move(temp.toPath(),menu.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);
            long deadline=SystemClock.elapsedRealtime()+2000;while(!cache.get(root,false).getString("revision").equals("second")&&SystemClock.elapsedRealtime()<deadline)Thread.sleep(5);
            check(cache.get(root,false).getString("revision").equals("second"),"Atomic native publication did not refresh cache");
            Files.write(menu.toPath(),"{\"revision\":\"command-fresh\"}".getBytes(StandardCharsets.UTF_8));check(cache.get(root,true).getString("revision").equals("command-fresh"),"Command admission used an old cache");
            Files.delete(menu.toPath());check(cache.get(root,true)==null,"Deleted native menu remained actionable");
        }finally{cache.close();menu.delete();temp.delete();root.delete();}
    }
    private void freshAdmission(Activity activity,String token)throws Exception {
        File root=new File(getTargetContext().getCacheDir(),"mcp-admission-fixture");root.mkdirs();File menu=new File(root,"native-menu.json");final int[] keys={0};
        Files.write(menu.toPath(),"{\"revision\":\"old\",\"input_sequence\":0,\"input_idle\":true}".getBytes(StandardCharsets.UTF_8));
        AndroidMcpSession binding=new AndroidMcpSession(()->root,()->true,()->true,()->true,k->{keys[0]++;return true;},()->{},"offline_fixture");
        runOnMainSync(()->AndroidMcp.bind(activity,binding));
        try{
            JSONObject before=call(token,"state",new JSONObject());
            Files.write(menu.toPath(),"{\"revision\":\"new\",\"input_sequence\":0,\"input_idle\":true}".getBytes(StandardCharsets.UTF_8));
            check(call(token,"key",key(root.getName(),"old",before.getLong("manual_generation"))).has("error")&&keys[0]==0,"Cached inspection weakened fresh command admission");
        }finally{runOnMainSync(binding::close);menu.delete();root.delete();}
    }
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;
        try{
            DisposableEmulator.require(getTargetContext());
            String installedVersion=DisposableEmulator.requireInstalledVersion(getTargetContext(),arguments);cacheTests();
            Intent intent=new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("mcp_test",true);activity=startActivitySync(intent);waitForIdleSync();Owner owner=new Owner();final Activity a=activity;
            runOnMainSync(()->{android.view.View notice=a.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(notice!=null)notice.performClick();});waitForIdleSync();
            runOnMainSync(()->{AndroidMcp.bind(a,owner);try{AndroidMcp.enable(a);}catch(Exception e){throw new RuntimeException(e);}});waitForIdleSync();
            String token=new String(Files.readAllBytes(new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").toPath()),StandardCharsets.US_ASCII);
            JSONObject state=call(token,"state",new JSONObject());long manual=state.getLong("manual_generation");check(state.getString("app_version").equals(installedVersion),"Installed version unavailable");check(state.getJSONArray("mcp_features").toString().contains("wait_state_v1"),"Transition capability absent");
            boolean denied=false;try{call("0".repeat(64),"state",new JSONObject());}catch(EOFException expected){denied=true;}check(denied,"Bad token was accepted");
            check(call(token,"key",key("wrong","fixture-menu",manual)).has("error")&&owner.keys==0,"Stale session admitted");
            check(call(token,"key",key(owner.session,"wrong",manual)).has("error")&&owner.keys==0,"Stale revision admitted");
            check(call(token,"key",key(owner.session,"fixture-menu",manual)).getBoolean("accepted")&&owner.keys==1,"Canonical key rejected");
            check(call(token,"key",key(owner.session,"fixture-menu",manual)).has("error")&&owner.keys==1,"Duplicate key before consumption admitted");
            runOnMainSync(()->owner.sequence++);check(call(token,"key",key(owner.session,"fixture-menu",manual)).getBoolean("accepted")&&owner.keys==2,"New consumed sequence rejected");
            runOnMainSync(AndroidMcp::manual);check(call(token,"key",key(owner.session,"fixture-menu",manual)).has("error")&&owner.keys==2,"Manual takeover ignored");
            runOnMainSync(()->owner.idle=false);state=call(token,"state",new JSONObject());check(call(token,"key",key(owner.session,"fixture-menu",state.getLong("manual_generation"))).has("error"),"Busy input admitted");
            JSONObject health=call(token,"health",new JSONObject());check(!health.getJSONObject("current").getBoolean("static_screen_is_failure"),"Static LCD became failure");check(!call(token,"crash_details",new JSONObject()).getBoolean("history_is_current_failure"),"OS history became current failure");
            runOnMainSync(()->owner.idle=true);state=call(token,"state",new JSONObject());
            int keysBefore=owner.keys;JSONObject timed=call(token,"wait_state",waitArgs(state,80));check(!timed.has("error"),"Timeout wait failed instead of returning observation: "+timed);check(!timed.getBoolean("observed_transition")&&owner.keys==keysBefore,"Transition wait manufactured a key/completion");
            check(call(token,"wait_state",waitArgs(state,1600)).has("error"),"Unbounded wait admitted");
            ExecutorService worker=Executors.newSingleThreadExecutor();
            try{
                final JSONObject before=state;Future<JSONObject> pending=worker.submit(()->call(token,"wait_state",waitArgs(before,1200)));Thread.sleep(80);
                long began=SystemClock.elapsedRealtime();call(token,"health",new JSONObject());long healthMs=SystemClock.elapsedRealtime()-began;
                check(!pending.isDone(),"Long poll blocked other endpoint clients");
                runOnMainSync(()->{owner.sequence++;owner.revision="next-menu";AndroidMcp.changed();});
                JSONObject completed=pending.get(2,TimeUnit.SECONDS);check(completed.getBoolean("observed_transition"),"Consumed key transition not observed");
                state=call(token,"state",new JSONObject());final JSONObject beforeManual=state;pending=worker.submit(()->call(token,"wait_state",waitArgs(beforeManual,1200)));Thread.sleep(50);runOnMainSync(AndroidMcp::manual);
                check(pending.get(2,TimeUnit.SECONDS).has("error")&&owner.keys==keysBefore,"Manual takeover failed to cancel wait");
                state=call(token,"state",new JSONObject());final JSONObject beforeStop=state;pending=worker.submit(()->call(token,"wait_state",waitArgs(beforeStop,1200)));Thread.sleep(50);
                check(call(token,"stop",new JSONObject().put("session",owner.session)).getBoolean("accepted")&&owner.stops==1,"Stop could not run during a transition wait");
                check(pending.get(2,TimeUnit.SECONDS).has("error"),"Stopped session wait survived");
                result.putLong("health_during_wait_ms",healthMs);result.putLong("observed_transition_wait_ms",completed.getLong("wait_elapsed_ms"));
            }finally{worker.shutdownNow();}
            freshAdmission(a,token);
            runOnMainSync(AndroidMcp::disable);check(!new File(getTargetContext().getNoBackupFilesDir(),"mcp-pairing-token").exists(),"Pairing token survived revocation");
            denied=false;try{call(token,"state",new JSONObject());}catch(IOException expected){denied=true;}check(denied,"Disabled endpoint accepted request");
            result.putString("stream","PASS: installed Android MCP endpoint: authentication, exact metadata, stale session/menu, consumed sequence, duplicate admission, manual takeover, busy input, bounded transition waits, concurrent health/Stop during waits, atomic menu-cache invalidation, fresh command reads, current health/history separation, revocation and private token deletion; synthetic owner, no firmware/vehicle commands\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}finally{final Activity a=activity;runOnMainSync(()->{AndroidMcp.disable();if(a!=null){AndroidMcp.unbind(a);a.finish();}});}finish(code,result);
    }
}
