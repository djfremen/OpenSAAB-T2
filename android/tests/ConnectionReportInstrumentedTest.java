// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.os.*;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.*;
import java.nio.file.*;
import org.json.*;

/** Emulator-only failure fixtures; no adapter access or report transmission. */
public final class ConnectionReportInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    boolean click(String text){
        AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();if(root==null)return false;
        for(AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText(text))
            if(node.isClickable() && node.performAction(AccessibilityNodeInfo.ACTION_CLICK))return true;
        return false;
    }
    public void onStart(){
        Context c=getTargetContext();Bundle result=new Bundle();int code=-1;Activity activity=null;
        File root=new File(c.getFilesDir(),"connection-attempts"),backup=new File(c.getFilesDir(),"connection-attempts-test-backup");
        try{
            check(!backup.exists(),"Previous test backup needs recovery");ConnectionAttempt.collect(c);
            if(root.exists())Files.move(root.toPath(),backup.toPath());
            ConnectionAttempt denied=new ConnectionAttempt(c,ConnectionAttempt.Adapter.CHIPSOFT);
            denied.device(0x483,0x5740);denied.stage(ConnectionAttempt.Stage.PERMISSION);
            denied.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.PERMISSION_DENIED);
            denied.stage(ConnectionAttempt.Stage.SESSION);denied.finish(ConnectionAttempt.Outcome.CANCELLED,ConnectionAttempt.Reason.USER_STOP);
            JSONArray attempts=ConnectionAttempt.collect(c);JSONObject d=attempts.getJSONObject(0);
            check(d.getString("stage").equals("PERMISSION") && d.getString("reason").equals("PERMISSION_DENIED"),"Failure overwritten by cleanup/cancel");
            String secret="TEST_SECRET_VIN_SEED_123456789";d.put("payload",secret).put("exception_message",secret);
            check(!ConnectionAttempt.safe(d).toString().contains(secret),"Unexpected fields exported");
            ConnectionAttempt timeout=new ConnectionAttempt(c,ConnectionAttempt.Adapter.NANO);
            timeout.stage(ConnectionAttempt.Stage.CHANNEL_OPEN);timeout.failure(new java.net.SocketTimeoutException(secret));
            JSONObject report=SupportReports.collect(c,"Connection test");String text=report.toString();
            check(text.contains("CHANNEL_OPEN")&&text.contains("TIMEOUT")&&!text.contains(secret),"Stage/timeout missing or secret leaked");
            for(int i=0;i<25;i++)new ConnectionAttempt(c,ConnectionAttempt.Adapter.SELECTION).finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.NO_ADAPTER);
            check(ConnectionAttempt.collect(c).length()==20,"Attempt retention is not bounded");
            for(String name:new String[]{"com.opensaab.usb.ChipsoftUsbActivity","com.opensaab.usb.NanoProbeActivity"}){
                activity=startActivitySync(new Intent().setClassName(c,name).putExtra("auto_start",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
                long end=SystemClock.elapsedRealtime()+5000;boolean offered=false;
                while(SystemClock.elapsedRealtime()<end){if(click("Report connection problem")){offered=true;break;}SystemClock.sleep(100);}
                check(offered,"Missing adapter failure has no reporting action: "+name);
                end=SystemClock.elapsedRealtime()+5000;boolean opened=false;
                while(SystemClock.elapsedRealtime()<end){AccessibilityNodeInfo screen=getUiAutomation().getRootInActiveWindow();if(screen!=null&&!screen.findAccessibilityNodeInfosByText("Report a problem").isEmpty()){opened=true;break;}SystemClock.sleep(100);}
                check(opened,"Reporting screen did not open");
                check(click("Back"),"Report screen has no Back");waitForIdleSync();
                Activity a=activity;runOnMainSync(a::finish);activity=null;waitForIdleSync();
            }
            check(ConnectionAttempt.collect(c).toString().contains("NO_ADAPTER"),"No-adapter UI failure not saved");
            result.putString("stream","PASS: terminal failure preserved, timed-out stage, secret exclusion, 20-attempt retention, Chipsoft and Nano no-adapter report action; no USB/network/email\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(activity!=null){Activity a=activity;runOnMainSync(a::finish);}
            ConnectionAttempt.collect(c);remove(root);try{if(backup.exists())Files.move(backup.toPath(),root.toPath());}catch(Exception e){code=0;result.putString("stream","FAIL restoring fixtures: "+e);}
            finish(code,result);
        }
    }
    void remove(File f){File[] children=f.listFiles();if(children!=null)for(File child:children)remove(child);f.delete();}
}
