// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;

import android.app.*;
import android.content.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import android.graphics.Bitmap;
import java.io.*;
import java.lang.reflect.*;

/** Run on a disposable emulator with independently configured size/density. */
public final class SetupLayoutInstrumentedTest extends Instrumentation {
    private Bundle args;
    public void onCreate(Bundle args) { super.onCreate(args); this.args=args; start(); }
    private Object field(MainActivity a,String name) throws Exception {
        Field f=MainActivity.class.getDeclaredField(name); f.setAccessible(true); return f.get(a);
    }
    private void require(boolean value,String message) { if(!value) throw new AssertionError(message); }
    public void onStart() {
        Bundle result=new Bundle();
        try {
            MainActivity a=(MainActivity)startActivitySync(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            Thread.sleep(1500);
            boolean expected=Boolean.parseBoolean(args.getString("wide"));
            require((Boolean)field(a,"wideLayout")==expected,"Unexpected responsive layout");
            FrameLayout viewport=(FrameLayout)field(a,"viewport");
            LinearLayout intro=(LinearLayout)field(a,"introduction"), installer=(LinearLayout)field(a,"installer");
            Button action=(Button)field(a,"action");
            require(action.getWidth()>0 && action.getHeight()>=48*a.getResources().getDisplayMetrics().density,"Button touch target too small: "+action.getWidth()+"x"+action.getHeight());
            require(intro.getWidth()>0 && installer.getWidth()>0,"Missing content");
            require(installer.getWidth()<=viewport.getWidth(),"Installation column overflows");
            int[] left=new int[2],right=new int[2];intro.getLocationOnScreen(left);installer.getLocationOnScreen(right);
            require(expected ? right[0]>left[0] : right[0]==left[0],"Wrong column placement");
            // Resize the actual Activity content during an in-progress operation.
            long until=SystemClock.elapsedRealtime()+15000;
            while(field(a,"selected")==null && SystemClock.elapsedRealtime()<until)Thread.sleep(100);
            Object selected=field(a,"selected"); require(selected!=null,"Catalog unavailable for state preservation check");
            runOnMainSync(()->{try {
                Field busy=MainActivity.class.getDeclaredField("busy");busy.setAccessible(true);busy.setBoolean(a,true);
                ((TextView)field(a,"state")).setText("Layout test: download in progress");
                ViewGroup.LayoutParams params=viewport.getLayoutParams();params.width=320;viewport.setLayoutParams(params);
            } catch(Exception e){throw new RuntimeException(e);}});
            Thread.sleep(500);
            require(!(Boolean)field(a,"wideLayout"),"Narrow window did not reflow");
            require(field(a,"selected")==selected && (Boolean)field(a,"busy"),"Resize lost operation state");
            require(field(a,"action")==action,"Resize replaced controls");
            runOnMainSync(()->{ViewGroup.LayoutParams params=viewport.getLayoutParams();params.width=-1;viewport.setLayoutParams(params);});
            Thread.sleep(500);
            require((Boolean)field(a,"wideLayout")==expected,"Expanded window did not reflow");
            try(FileOutputStream out=new FileOutputStream(new File(getTargetContext().getExternalFilesDir(null),"setup-layout-"+args.getString("label")+".png"))) {
                getUiAutomation().takeScreenshot().compress(Bitmap.CompressFormat.PNG,100,out);
            }
            result.putString("stream","PASS: "+args.getString("label")+" layout, touch target, column placement and in-progress resize state preservation");
            finish(Activity.RESULT_OK,result);
        } catch(Throwable e) { result.putString("stream","FAIL: "+e);finish(Activity.RESULT_CANCELED,result); }
    }
}
