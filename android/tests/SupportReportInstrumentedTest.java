// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.Uri;
import android.os.*;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;
import org.json.*;

/** Isolated Android emulator fixtures; no USB, vehicle, network or message sending. */
public final class SupportReportInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    boolean click(String text){
        AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();if(root==null)return false;
        for(AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText(text))if(node.isClickable()&&node.performAction(AccessibilityNodeInfo.ACTION_CLICK))return true;
        return false;
    }
    public void onStart(){
        Bundle result=new Bundle();int code=-1;Activity activity=null;File dir=null;
        Context c=getTargetContext();File errors=new File(c.getFilesDir(),"last-app-error.json");byte[] old=null;
        File reports=new File(c.getFilesDir(),"support-reports");Set<String> existing=new HashSet<>();if(reports.list()!=null)Collections.addAll(existing,reports.list());
        try{
            if(errors.exists())old=Files.readAllBytes(errors.toPath());
            dir=new File(c.getFilesDir(),"chipsoft-"+UUID.randomUUID());check(dir.mkdir(),"Fixture directory missing");
            String secret="TEST_VIN_AND_SSA_SECRET_9123456789"; // gitleaks:allow -- synthetic redaction test marker
            File log=new File(dir,"usb.log");
            try(FileOutputStream out=new FileOutputStream(log)){
                out.write(("ERROR old event outside tail\n").getBytes("UTF-8"));
                byte[] padding=new byte[300000];Arrays.fill(padding,(byte)'x');out.write(padding);
                out.write(("\nUSB_OPEN "+secret+"\nUSB_TX "+secret+"\nTIMEOUT\n").getBytes("UTF-8"));
            }
            JSONObject summary=SupportReports.logSummary(log),counts=summary.getJSONObject("event_counts");
            check(summary.getBoolean("tail_only")&&counts.getInt("USB_OPEN")==1&&counts.getInt("TIMEOUT")==1&&!counts.has("ERROR"),"Bounded tail counts wrong");
            Files.write(new File(dir,"report.json").toPath(),new JSONObject().put("status","incomplete").put("exit_code",3).put("instructions",120000000).put("reason",secret).put("card_image",secret).toString().getBytes("UTF-8"));
            SupportReports.recordError(c,new IOException(secret),false);
            JSONObject report=SupportReports.collect(c,"Tested an adapter timeout");String json=report.toString();
            check(!json.contains(secret)&&!json.contains("xxxxxxxx")&&json.contains("java.io.IOException"),"Sensitive log/message exported or error absent");
            check(report.getJSONArray("recent_sessions").length()>0&&json.contains("emulator_outcome"),"Missing session/outcome");
            File saved=SupportReports.save(c,report);
            try(ZipFile zip=new ZipFile(saved)){
                check(zip.size()==1&&zip.getEntry("diagnostics.json")!=null,"Unexpected ZIP contents");
                try(InputStream input=zip.getInputStream(zip.getEntry("diagnostics.json"))){ByteArrayOutputStream b=new ByteArrayOutputStream();byte[] buf=new byte[4096];int n;while((n=input.read(buf))!=-1)b.write(buf,0,n);check(!b.toString("UTF-8").contains(secret),"Secret in shared ZIP");}
            }
            Intent share=SupportReportActivity.sharingIntent(c,saved);Uri uri=share.getParcelableExtra(Intent.EXTRA_STREAM);
            check(Intent.ACTION_SEND.equals(share.getAction())&&"application/zip".equals(share.getType()),"Wrong share intent");
            check(!share.hasExtra(Intent.EXTRA_EMAIL)&&share.getClipData().getItemAt(0).getUri().equals(uri),"Recipient guessed or grant omitted");
            check((share.getFlags()&Intent.FLAG_GRANT_READ_URI_PERMISSION)!=0&&(share.getFlags()&Intent.FLAG_GRANT_WRITE_URI_PERMISSION)==0,"Wrong URI grant");
            try(ParcelFileDescriptor fd=c.getContentResolver().openFileDescriptor(uri,"r")){check(fd.getStatSize()==saved.length(),"Read provider failed");}
            try{c.getContentResolver().openFileDescriptor(uri,"w");throw new AssertionError("Writable provider");}catch(FileNotFoundException expected){}
            for(String name:new String[]{"../firmware/card.bin","usb.log","%2e%2e%2ffirmware%2fcard.bin"}){
                Uri bad=new Uri.Builder().scheme("content").authority(c.getPackageName()+".support-reports").appendPath(name).build();
                try{c.getContentResolver().openFileDescriptor(bad,"r");throw new AssertionError("Traversal allowed");}catch(FileNotFoundException expected){}
            }
            activity=startActivitySync(new Intent().setClassName(c,"com.opensaab.usb.SupportReportActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
            long ready=SystemClock.elapsedRealtime()+5000;boolean prepared=false;while(SystemClock.elapsedRealtime()<ready){if(click("Prepare report")){prepared=true;break;}SystemClock.sleep(100);}check(prepared,"Prepare action missing");long until=SystemClock.elapsedRealtime()+5000;boolean review=false;
            while(SystemClock.elapsedRealtime()<until){if(click("Keep private")){review=true;break;}SystemClock.sleep(100);}
            check(review,"Review/keep private action missing");
            result.putString("stream","PASS: bounded log tails, secret/message exclusion, ZIP allowlist, Android exit metadata, review/keep-private UI, read-only share grants, traversal denial; no vehicle/network/mail\n");
        }catch(Throwable e){code=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{
            if(activity!=null){Activity a=activity;runOnMainSync(a::finish);}
            try{if(old==null)errors.delete();else Files.write(errors.toPath(),old);}catch(Exception ignored){}
            if(dir!=null){File[] files=dir.listFiles();if(files!=null)for(File f:files)f.delete();dir.delete();}
            if(reports.listFiles()!=null)for(File f:reports.listFiles())if(!existing.contains(f.getName()))f.delete();
            finish(code,result);
        }
    }
}
