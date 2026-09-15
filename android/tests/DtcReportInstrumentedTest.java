// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.Uri;
import android.os.*;
import android.view.*;
import android.graphics.Rect;
import java.io.*;
import java.nio.file.*;

/** Synthetic LCD fixture only. Never opens USB or sends a diagnostic/key command. */
public final class DtcReportInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    void main(Runnable r){Throwable[] failure={null};runOnMainSync(()->{try{r.run();}catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
    void awaitReport(ChipsoftUsbActivity a,String count){
        long started=SystemClock.elapsedRealtime();
        while(SystemClock.elapsedRealtime()-started<5000){
            boolean[] ready={false};main(()->ready[0]=a.dtcReport.isShown()&&a.dtcReport.getContentDescription().toString().contains(count));
            if(ready[0]){android.util.Log.i("OpenSaabDtcTest","Observed report "+count+" after "+(SystemClock.elapsedRealtime()-started)+" ms");return;}
            SystemClock.sleep(100);
        }
        throw new AssertionError("Report did not reach "+count+" within 5 seconds");
    }
    void returnToCodes(){
        long until=SystemClock.elapsedRealtime()+5000;
        while(SystemClock.elapsedRealtime()<until){
            android.view.accessibility.AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();
            if(root!=null)for(android.view.accessibility.AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText("Back to codes")){
                if(node.isClickable()&&node.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)){waitForIdleSync();return;}
            }
            SystemClock.sleep(100);
        }
        throw new AssertionError("Report review Back to codes button not accessible");
    }
    static String screen(int i,int n){return "            DTC Information             \n\nACC       B0260  06 Vent Door Motor Left\nICM2      B1000  08 Control Module. Inte\nICM2      B1000  08 Control Module. Inte\n\n                               "+i+" / "+n+"\n";}
    public void onStart(){
        Bundle result=new Bundle();int code=-1;ChipsoftUsbActivity activity=null;File dir=null;
        java.util.Set<String> existing=new java.util.HashSet<>();File reports=new File(getTargetContext().getFilesDir(),"dtc-reports");
        if(reports.list()!=null)java.util.Collections.addAll(existing,reports.list());
        try{
            activity=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final ChipsoftUsbActivity a=activity;waitForIdleSync();
            dir=Files.createTempDirectory(a.getCacheDir().toPath(),"dtc-ui-test-").toFile();final File run=dir;
            VehicleSession.save(new File(dir,VehicleSession.FILE),new VehicleIdentity("YS3FD49YX41000001","2026-09-10T00:00:00Z","test fixture",2004,"available","9440","B207R","Silver","FA57","Sedan"));
            File screen=new File(dir,"native-dtc-screen.txt");
            Files.write(screen.toPath(),"DTC Information\nACC".getBytes("UTF-8"));main(()->a.nativeDirectory=run);SystemClock.sleep(800);
            main(()->check(a.dtcReport.getVisibility()==View.GONE,"Progress screen falsely captured"));
            Files.write(screen.toPath(),screen(2,3).getBytes("UTF-8"));awaitReport(a,"1/3");
            main(()->{
                check(a.dtcReport.isShown()&&a.dtcReport.getContentDescription().toString().contains("1/3"),"Partial report not detected");
                View exit=a.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect r=new Rect();check(exit.getGlobalVisibleRect(r)&&r.height()==exit.getHeight(),"Report hid EXIT");
                a.dtcReport.performClick();
            });waitForIdleSync();returnToCodes();
            main(()->check(!a.isFinishing()&&!a.isDestroyed(),"Review close finished diagnostics activity"));
            Files.write(screen.toPath(),screen(1,3).getBytes("UTF-8"));awaitReport(a,"2/3");
            Files.write(screen.toPath(),screen(3,3).getBytes("UTF-8"));awaitReport(a,"3/3");
            main(()->check(a.dtcReport.getContentDescription().toString().contains("3/3"),"Full list not captured"));
            Files.write(screen.toPath(),"DTC Information\nICM2\nB1000 08\nControl Module. Internal fault\n".getBytes("UTF-8"));SystemClock.sleep(1000);
            Files.write(screen.toPath(),screen(3,3).getBytes("UTF-8"));SystemClock.sleep(1000);
            main(()->check(a.dtcReport.getContentDescription().toString().contains("3/3"),"Opening code details reset captured coverage"));
            check(!new File(dir,"native-key.txt").exists()&&!a.running.get(),"Observer sent key or started firmware");
            File[] saved=reports.listFiles((d,n)->n.endsWith(".txt")&&!existing.contains(n));check(saved!=null&&saved.length==1,"Duplicate report files");
            String txt=new String(Files.readAllBytes(saved[0].toPath()),"UTF-8");check(txt.contains("All 3 entries")&&txt.contains("ICM2"),"Report content missing");
            check(txt.contains("VIN: YS3FD49YX41000001"),"Observer did not attach session VIN");
            check(txt.contains("Control Module. Internal fault")&&!txt.contains("--- Selected item"),"Readable report lost details or contains raw dumps");
            Uri uri=new Uri.Builder().scheme("content").authority(a.getPackageName()+".dtc-reports").appendPath(saved[0].getName()).build();
            Intent share=DtcReportView.sharingIntent(a,saved[0],"test summary","chipsoft");
            check(Intent.ACTION_SEND.equals(share.getAction()) && share.getParcelableExtra(Intent.EXTRA_STREAM).equals(uri),"Sharing omitted report");
            check((share.getFlags()&Intent.FLAG_GRANT_READ_URI_PERMISSION)!=0 && (share.getFlags()&Intent.FLAG_GRANT_WRITE_URI_PERMISSION)==0,"Wrong sharing grant");
            check(share.getClipData().getItemAt(0).getUri().equals(uri)&&!share.hasExtra(Intent.EXTRA_EMAIL),"Sharing must let user choose recipient");
            org.json.JSONObject json=new org.json.JSONObject(new String(Files.readAllBytes(new File(saved[0].getPath().replace(".txt",".json")).toPath()),"UTF-8"));
            check(json.getJSONArray("snapshots").length()==3 && json.getJSONArray("snapshots").getJSONObject(0).getJSONArray("visible_rows").length()==3,"JSON lost duplicate rows");
            check(json.getString("vin").equals("YS3FD49YX41000001"),"Saved JSON lost session VIN");
            try(ParcelFileDescriptor fd=a.getContentResolver().openFileDescriptor(uri,"r")){check(fd.getStatSize()==saved[0].length(),"Shared file size wrong");}
            try{a.getContentResolver().openFileDescriptor(uri,"w");throw new AssertionError("Provider allowed writes");}catch(FileNotFoundException expected){}
            for(String path:new String[]{"../firmware/eprom.bin","native-chipsoft.log","%2e%2e%2ffirmware%2feprom.bin"}){
                Uri bad=new Uri.Builder().scheme("content").authority(a.getPackageName()+".dtc-reports").appendPath(path).build();
                try{a.getContentResolver().openFileDescriptor(bad,"r");throw new AssertionError("Provider exposed "+path);}catch(FileNotFoundException expected){}
            }
            Files.write(screen.toPath(),"Main Menu\n".getBytes("UTF-8"));SystemClock.sleep(1500);
            main(()->check(a.dtcReport.isShown(),"Saved report disappeared on exit"));
            Files.write(screen.toPath(),screen(1,3).getBytes("UTF-8"));awaitReport(a,"1/3");
            main(()->check(a.dtcReport.getContentDescription().toString().contains("1/3"),"Separate scan merged with old report"));
            result.putString("stream","PASS: numbered DTC detection, partial/full coverage, durable text/JSON, review dialog, persistent EXIT, separate scans, read-only sharing provider, traversal rejection; no USB/keys/network/mail\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(activity!=null){final Activity a=activity;main(a::finish);}SystemClock.sleep(300);
            if(dir!=null){for(File f:dir.listFiles())f.delete();dir.delete();}
            if(reports.listFiles()!=null)for(File f:reports.listFiles())if(!existing.contains(f.getName()))f.delete();
            finish(code,result);
        }
    }
}
