// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import android.graphics.*;
import java.io.*;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

/** Installed launch gate, report navigation, rotation and repeat launch; no upload. */
public final class PreviewWelcomeInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args){super.onCreate(args);start();}
    private void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    private View tag(Activity a,String value){return a.findViewById(android.R.id.content).findViewWithTag(value);}
    private String text(Activity a,String value){View v=tag(a,value);check(v instanceof TextView,"Missing "+value);return ((TextView)v).getText().toString();}
    private Intent launch(){return new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addCategory(Intent.CATEGORY_LAUNCHER).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);}
    public void onStart(){Bundle result=new Bundle();Activity activity=null;int code=0;
        try{
            check(SimulatorVehicleConnection.available(getTargetContext()),"Disposable emulator required");
            activity=startActivitySync(launch());waitForIdleSync();final Activity first=activity;
            runOnMainSync(()->{
                check(tag(first,"developer-preview-welcome")!=null&&tag(first,"session-workspace")==null,"App workspace preceded welcome");
                check(text(first,"preview-title").equals("Developer Preview"),"Preview label absent");
                check(text(first,"preview-version").contains("0.1.0-preview.90")&&text(first,"preview-version").contains("ARM64"),"Installed identity absent");
                check(text(first,"preview-release-date").contains("2026")&&text(first,"preview-release-date").startsWith("Version date: ")&&!text(first,"preview-release-date").toLowerCase(java.util.Locale.ROOT).contains("private"),"Packaged version date absent or confusing private label present");
                check(text(first,"preview-message").contains("Report issue")&&text(first,"preview-review-note").contains("review"),"Development/report invitation absent");
                check(!FirmwareGate.sessionActive(),"Welcome started diagnostics");
                View root=first.getWindow().getDecorView();Bitmap capture=Bitmap.createBitmap(root.getWidth(),root.getHeight(),Bitmap.Config.ARGB_8888);root.draw(new Canvas(capture));
                try(FileOutputStream out=new FileOutputStream(new File(getTargetContext().getCacheDir(),"preview-welcome.png"))){capture.compress(Bitmap.CompressFormat.PNG,100,out);}catch(IOException e){throw new RuntimeException(e);}finally{capture.recycle();}
            });
            ActivityMonitor report=addMonitor("com.opensaab.usb.SupportReportActivity",null,false);
            runOnMainSync(()->tag(first,"preview-report-issue").performClick());
            Activity form=waitForMonitorWithTimeout(report,5000);check(form!=null,"Report issue did not open the existing form");removeMonitor(report);
            runOnMainSync(form::finish);waitForIdleSync();
            check(tag(first,"developer-preview-welcome")!=null&&tag(first,"session-workspace")==null,"Returning from reporting continued automatically");
            runOnMainSync(()->tag(first,"preview-continue").performClick());waitForIdleSync();
            check(tag(first,"session-workspace")!=null&&tag(first,"developer-preview-welcome")==null,"Continue did not reach common app workspace");
            AtomicReference<Activity> restored=new AtomicReference<>();Application app=(Application)getTargetContext().getApplicationContext();
            Application.ActivityLifecycleCallbacks callbacks=new Application.ActivityLifecycleCallbacks(){
                public void onActivityResumed(Activity a){if(a!=first&&a.getClass()==first.getClass())restored.set(a);}
                public void onActivityCreated(Activity a,Bundle b){} public void onActivityStarted(Activity a){} public void onActivityPaused(Activity a){} public void onActivityStopped(Activity a){} public void onActivitySaveInstanceState(Activity a,Bundle b){} public void onActivityDestroyed(Activity a){}
            };
            app.registerActivityLifecycleCallbacks(callbacks);runOnMainSync(first::recreate);
            long limit=SystemClock.elapsedRealtime()+5000;while(restored.get()==null&&SystemClock.elapsedRealtime()<limit)Thread.sleep(30);
            app.unregisterActivityLifecycleCallbacks(callbacks);activity=restored.get();check(activity!=null,"Activity restoration failed");
            check(tag(activity,"session-workspace")!=null&&tag(activity,"developer-preview-welcome")==null,"Rotation repeated the launch notice");
            Activity rotated=activity;runOnMainSync(rotated::finish);waitForIdleSync();
            activity=startActivitySync(launch());waitForIdleSync();
            check(tag(activity,"developer-preview-welcome")!=null&&tag(activity,"session-workspace")==null,"New launch did not repeat notice");
            JSONObject receipt=new JSONObject().put("result","pass").put("version",AppBuildProfile.installedVersion(activity)).put("date_line",text(activity,"preview-release-date"))
                .put("every_fresh_launch",true).put("report_form_return",true).put("continue_workspace",true).put("rotation_preserves_continue",true).put("automatic_upload",false);
            FirmwareStore.writeJson(new File(getTargetContext().getCacheDir(),"preview-welcome-receipt.json"),receipt);
            result.putString("stream","PASS: exact installed Developer Preview/full version/ARM64/packaged version date with no private label, before workspace/diagnostics; existing Report issue form and return, Continue, rotation preservation and every fresh launch; no report submission\n");code=-1;
        }catch(Throwable error){result.putString("stream","FAIL: "+error+"\n");}
        finally{Activity last=activity;if(last!=null)runOnMainSync(last::finish);}
        finish(code,result);
    }
}
