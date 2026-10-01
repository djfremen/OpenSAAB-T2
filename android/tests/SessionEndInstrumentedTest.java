// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.graphics.Bitmap;
import android.os.*;
import java.io.File;
import java.lang.reflect.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Synthetic native-presentation bindings; real transport teardown needs hardware. */
public final class SessionEndInstrumentedTest extends Instrumentation {
    private Activity active;
    static void check(boolean yes,String why){if(!yes)throw new AssertionError(why);}
    static Field field(Object o,String name)throws Exception {Class<?> c=o.getClass();Field f=c.getDeclaredField(name);f.setAccessible(true);return f;}
    static void call(Object o,String name,Class<?>[] types,Object... args)throws Exception {Method m=o.getClass().getDeclaredMethod(name,types);m.setAccessible(true);m.invoke(o,args);}
    public void onCreate(Bundle b){super.onCreate(b);start();}
    public void onStart(){Bundle result=new Bundle();int code=-1;
        try{
            active=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            Object lcd=field(active,"lcd").get(active);Field frame=field(lcd,"frame");
            field(active,"compactSecurity").set(active,"Security: PRE-AUTH · synthetic saved data");
            for(String reason:new String[]{"Offline menus stopped","Offline session ended","Emulation stopped unexpectedly (exit 7)","Cannot run firmware: synthetic startup failure","Stopped in background"}){
                runOnMainSync(()->{try{field(active,"running").setBoolean(active,true);frame.set(lcd,Bitmap.createBitmap(320,240,Bitmap.Config.ARGB_8888));call(active,"finishSession",new Class[]{String.class},reason);}catch(Exception e){throw new RuntimeException(e);}});
                check(frame.get(lcd)==null&&!field(active,"running").getBoolean(active),"End left live display: "+reason);
                check(((android.widget.TextView)field(active,"status").get(active)).getText().toString().equals(reason),"End reason lost");
                Object workspace=field(active,"workspace").get(active);android.widget.TextView summary=(android.widget.TextView)field(workspace,"summary").get(workspace);
                check(summary.getText().toString().startsWith(reason),"Saved security history hid end reason");
            }
            runOnMainSync(active::finish);
            active=startActivitySync(new Intent(getTargetContext(),ChipsoftUsbActivity.class).putExtra("native_firmware",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            File ended=new File(getTargetContext().getCacheDir(),"synthetic-end");
            runOnMainSync(()->{try{field(active,"nativeDirectory").set(active,ended);((AtomicBoolean)field(active,"running").get(active)).set(false);android.widget.ImageView v=(android.widget.ImageView)field(active,"nativeLcd").get(active);v.setImageBitmap(Bitmap.createBitmap(320,240,Bitmap.Config.ARGB_8888));call(active,"finishNativePresentation",new Class[]{File.class,String.class},ended,"Live adapter stopped");}catch(Exception e){throw new RuntimeException(e);}});
            check(((android.widget.ImageView)field(active,"nativeLcd").get(active)).getDrawable()==null,"Chipsoft end left frame");
            Object workspace=field(active,"workspace").get(active);check(((android.widget.TextView)field(workspace,"summary").get(workspace)).getText().toString().startsWith("Live adapter stopped"),"Chipsoft reason hidden in details");
            runOnMainSync(active::finish);
            active=startActivitySync(new Intent(getTargetContext(),NanoProbeActivity.class).putExtra("native_dtc",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            runOnMainSync(()->{try{android.widget.ImageView v=(android.widget.ImageView)field(active,"nativeLcd").get(active);v.setImageBitmap(Bitmap.createBitmap(320,240,Bitmap.Config.ARGB_8888));call(active,"clearNativePresentation",new Class[]{});}catch(Exception e){throw new RuntimeException(e);}});
            check(((android.widget.ImageView)field(active,"nativeLcd").get(active)).getDrawable()==null,"Nano end left frame");
            result.putString("stream","PASS: offline Stop/completion/unexpected-exit/startup/lifecycle presentation clears and preserves reasons; Chipsoft/Nano terminal display bindings clear. Synthetic states, no firmware/USB/vehicle traffic; transport teardown unqualified.\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(active!=null)runOnMainSync(active::finish);finish(code,result);}
    }
}
