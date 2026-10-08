// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.*;
import android.content.pm.ActivityInfo;
import android.content.res.Configuration;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.util.concurrent.atomic.AtomicReference;

/** Real Activity lifecycle transitions; synthetic draft only, no uploads or adapter. */
public final class ReportLifecycleInstrumentedTest extends Instrumentation {
    static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    static <T> T field(Activity a,String name,Class<T> type)throws Exception {
        java.lang.reflect.Field f=SupportReportActivity.class.getDeclaredField(name);f.setAccessible(true);return type.cast(f.get(a));
    }
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){
        Bundle result=new Bundle();AtomicReference<Activity> current=new AtomicReference<>();
        java.util.concurrent.atomic.AtomicInteger pauses=new java.util.concurrent.atomic.AtomicInteger(),resumes=new java.util.concurrent.atomic.AtomicInteger();
        Application app=(Application)getTargetContext().getApplicationContext();
        Application.ActivityLifecycleCallbacks callbacks=new Application.ActivityLifecycleCallbacks(){
            public void onActivityResumed(Activity a){if(a instanceof SupportReportActivity){current.set(a);resumes.incrementAndGet();}}
            public void onActivityCreated(Activity a,Bundle b){} public void onActivityStarted(Activity a){}
            public void onActivityPaused(Activity a){if(a instanceof SupportReportActivity)pauses.incrementAndGet();} public void onActivityStopped(Activity a){}
            public void onActivitySaveInstanceState(Activity a,Bundle b){} public void onActivityDestroyed(Activity a){}
        };
        app.registerActivityLifecycleCallbacks(callbacks);
        try{
            Activity a=startActivitySync(new Intent(getTargetContext(),SupportReportActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final EditText description=field(a,"description",EditText.class),contact=field(a,"contact",EditText.class);
            final Spinner context=field(a,"testContext",Spinner.class);final CheckBox consent=field(a,"allowContact",CheckBox.class);
            check(!consent.isChecked(),"Consent enabled by default");
            runOnMainSync(()->{description.setText("Synthetic lifecycle draft");contact.setText("synthetic@example.invalid");context.setSelection(2);});
            for(int orientation:new int[]{ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE,ActivityInfo.SCREEN_ORIENTATION_PORTRAIT}){
                Activity before=current.get();int expected=orientation==ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE?Configuration.ORIENTATION_LANDSCAPE:Configuration.ORIENTATION_PORTRAIT;
                runOnMainSync(()->before.setRequestedOrientation(orientation));
                long end=SystemClock.elapsedRealtime()+8000;
                while(SystemClock.elapsedRealtime()<end){waitForIdleSync();a=current.get();if(a!=null&&!a.isDestroyed()&&a.getResources().getConfiguration().orientation==expected)break;SystemClock.sleep(50);}
                check(a.getResources().getConfiguration().orientation==expected,"Orientation did not change");assertDraft(a);
            }
            Activity before=current.get();runOnMainSync(before::recreate);
            long end=SystemClock.elapsedRealtime()+8000;
            while(SystemClock.elapsedRealtime()<end&&(current.get()==before||current.get().isDestroyed()))SystemClock.sleep(50);
            waitForIdleSync();check(current.get()!=before,"Activity did not recreate");assertDraft(current.get());
            // Home backgrounds the task; bringing the same report task forward must preserve the draft.
            int paused=pauses.get(),resumed=resumes.get();
            getTargetContext().startActivity(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            end=SystemClock.elapsedRealtime()+5000;while(pauses.get()==paused&&SystemClock.elapsedRealtime()<end)SystemClock.sleep(50);
            check(pauses.get()>paused,"Home did not pause report screen");
            getTargetContext().startActivity(new Intent(getTargetContext(),SupportReportActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_REORDER_TO_FRONT));
            end=SystemClock.elapsedRealtime()+5000;while(resumes.get()==resumed&&SystemClock.elapsedRealtime()<end)SystemClock.sleep(50);
            check(resumes.get()>resumed,"Report did not resume");
            waitForIdleSync();assertDraft(current.get());
            check(!new java.io.File(getTargetContext().getFilesDir(),"last-support-upload.json").exists(),"Unexpected upload receipt");
            result.putString("stream","PASS: actual landscape/portrait rotation, Activity recreation, Home/resume preserve draft/context and unchecked contact consent; no upload or adapter commands\n");finish(-1,result);
        }catch(Throwable e){result.putString("stream","FAIL: "+e+"\n");finish(0,result);}
        finally{app.unregisterActivityLifecycleCallbacks(callbacks);Activity a=current.get();if(a!=null)runOnMainSync(a::finish);}
    }
    static void assertDraft(Activity a)throws Exception{
        check(field(a,"description",EditText.class).getText().toString().equals("Synthetic lifecycle draft"),"Description lost");
        check(field(a,"contact",EditText.class).getText().toString().equals("synthetic@example.invalid"),"Contact draft lost");
        check(field(a,"testContext",Spinner.class).getSelectedItemPosition()==2,"Testing context lost");
        check(!field(a,"allowContact",CheckBox.class).isChecked(),"Consent changed during lifecycle transition");
    }
}
