// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.os.*;
import java.io.File;
import java.util.concurrent.*;
import org.json.JSONObject;

/** Local health checks, never vehicle commands or automatic uploads/restarts. */
public final class EmulatorHealthMonitor implements AutoCloseable,Application.ActivityLifecycleCallbacks {
    private final Activity activity;
    private final Runnable stop;
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final ScheduledExecutorService worker=Executors.newSingleThreadScheduledExecutor(r->new Thread(r,"emulator-health"));
    private final EmulatorHealthState state=new EmulatorHealthState();
    private File directory; private long stamp, generation; private boolean foreground=true,closed,uiPending;
    private AlertDialog dialog;
    public EmulatorHealthMonitor(Activity a,Runnable stop){activity=a;this.stop=stop;a.getApplication().registerActivityLifecycleCallbacks(this);worker.scheduleWithFixedDelay(this::tick,1,1,TimeUnit.SECONDS);}
    public synchronized void begin(File run){if(closed)return;directory=run;stamp=0;generation++;state.reset(SystemClock.uptimeMillis());}
    public synchronized void input(){if(directory!=null)state.input(SystemClock.uptimeMillis());}
    public synchronized void frame(){state.frame();}
    public synchronized void expectedStop(){directory=null;generation++;}
    public synchronized void ended(){if(directory==null)return;incident("unexpected_emulator_exit");directory=null;}
    private synchronized void tick(){
        if(closed||directory==null||!foreground)return;
        long now=SystemClock.uptimeMillis();
        File beat=new File(directory,"emulator-heartbeat");long changed=beat.lastModified();
        if(changed!=0&&changed!=stamp){stamp=changed;state.heartbeat(now);}
        String reason=state.reason(now);if(reason!=null)incident(reason);
        if(!uiPending){uiPending=true;ui.post(()->{synchronized(this){uiPending=false;state.ui(SystemClock.uptimeMillis());}});}
    }
    private void incident(String reason){
        if(state.alerted||closed)return;state.alerted=true;
        long now=SystemClock.uptimeMillis(),epoch=generation;
        try{JSONObject event=new JSONObject().put("utc",java.time.Instant.now().toString()).put("reason",reason)
            .put("elapsed_ms",now-state.started).put("heartbeat_age_ms",now-state.heartbeat)
            .put("input_wait_ms",state.input<0?0:now-state.input).put("ui_delay_ms",now-state.ui);
            FirmwareStore.writeJson(new File(activity.getFilesDir(),"last-emulator-health.json"),event);
        }catch(Exception ignored){}
        ui.post(()->{synchronized(this){if(closed||!foreground||epoch!=generation||activity.isFinishing()||activity.isDestroyed())return;}
            activity.getSharedPreferences("health-prompts",0).edit().putLong("health_offered",new File(activity.getFilesDir(),"last-emulator-health.json").lastModified()).apply();
            dialog=new AlertDialog.Builder(activity).setTitle(reason.equals("unexpected_emulator_exit")?"Emulation stopped unexpectedly":"Emulation may be unresponsive")
                .setMessage("Would you like to send a report to OpenSAAB? A slow operation can also cause this warning. Nothing has been uploaded or restarted.\n\nYou can review the report before sending it. Choosing Send report stops this session first.")
                .setPositiveButton("Send report to OpenSAAB",(d,w)->{expectedStop();stop.run();activity.startActivity(new Intent(activity,SupportReportActivity.class).putExtra("health_report",true));})
                .setNegativeButton("Keep waiting",(d,w)->{}).show();
        });
    }
    public synchronized void close(){closed=true;directory=null;generation++;worker.shutdownNow();activity.getApplication().unregisterActivityLifecycleCallbacks(this);if(dialog!=null)dialog.dismiss();}
    public synchronized void onActivityResumed(Activity a){if(a==activity){foreground=true;state.ui(SystemClock.uptimeMillis());}}
    public synchronized void onActivityPaused(Activity a){if(a==activity){foreground=false;state.frame();}}
    public void onActivityDestroyed(Activity a){if(a==activity)close();}
    public void onActivityCreated(Activity a,Bundle b){} public void onActivityStarted(Activity a){} public void onActivityStopped(Activity a){} public void onActivitySaveInstanceState(Activity a,Bundle b){}
    public static void offerPreviousCrash(Activity a){
        if(a instanceof SupportReportActivity)return;
        File crash=new File(a.getFilesDir(),"last-app-crash.json"),health=new File(a.getFilesDir(),"last-emulator-health.json");
        boolean isCrash=crash.lastModified()>=health.lastModified();File file=isCrash?crash:health;long modified=file.lastModified();String key=isCrash?"crash_offered":"health_offered";
        android.content.SharedPreferences prefs=a.getSharedPreferences("health-prompts",0);
        if(modified==0||prefs.getLong(key,0)==modified)return;
        prefs.edit().putLong(key,modified).apply();
        new AlertDialog.Builder(a).setTitle(isCrash?"OpenSAAB closed unexpectedly":"Previous emulation problem")
            .setMessage("Send a report to help investigate the crash? You can review it before sending. Nothing is uploaded automatically.")
            .setPositiveButton("Send report to OpenSAAB",(d,w)->a.startActivity(new Intent(a,SupportReportActivity.class).putExtra("health_report",true)))
            .setNegativeButton("Not now",null).show();
    }
}
