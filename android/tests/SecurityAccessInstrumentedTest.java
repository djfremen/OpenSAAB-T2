// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.Intent;
import android.graphics.Rect;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.nio.file.*;

/** Uses a private synthetic prompt; no API requests, USB or card import. */
public final class SecurityAccessInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    void main(Runnable r){Throwable[] failure={null};runOnMainSync(()->{try{r.run();}catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
    public void onStart(){
        Bundle result=new Bundle();int code=-1;ChipsoftUsbActivity activity=null;File dir=null;
        try{
            activity=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity a=activity;waitForIdleSync();
            dir=Files.createTempDirectory(a.getCacheDir().toPath(),"security-ui-test-").toFile();final File run=dir;
            File screen=new File(dir,"native-dtc-screen.txt");
            Files.write(screen.toPath(),"F6: Get Security Access".getBytes("UTF-8"));
            main(()->{a.nativeDirectory=run;a.securityAccess.refresh();});SystemClock.sleep(300);
            main(()->check(a.securityAccess.getVisibility()==View.GONE,"Menu label falsely detected as request"));
            Files.write(screen.toPath(),"Help\nYou need Security Access from TIS2000\n1. Disconnect Tech 2 from Vehicle.".getBytes("UTF-8"));
            main(()->a.securityAccess.refresh());SystemClock.sleep(400);
            main(()->{
                check(a.securityAccess.isShown(),"TIS prompt action missing");
                Button action=(Button)a.securityAccess.getChildAt(1);check(action.getText().toString().equals("Get security access"),"Wrong next action");
                View exit=a.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect r=new Rect();check(exit.getGlobalVisibleRect(r)&&r.height()==exit.getHeight(),"Security banner hid EXIT");
                check(!a.running.get()&&!SecurityAccessView.workflowBusy(),"Detection initiated a session or request");
                action.performClick(); // Opens an explanation only, never taps Start collection.
            });waitForIdleSync();sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);waitForIdleSync();
            main(()->check(!a.running.get()&&!SecurityAccessView.workflowBusy(),"Dismissing security dialog initiated work"));
            result.putString("stream","PASS: TIS help detected, menu label ignored, explicit security action, EXIT remains visible, dismiss has no side effect; no USB/API/card operations\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(activity!=null){final Activity a=activity;main(a::finish);}if(dir!=null){for(File f:dir.listFiles())f.delete();dir.delete();}finish(code,result);}
    }
}
