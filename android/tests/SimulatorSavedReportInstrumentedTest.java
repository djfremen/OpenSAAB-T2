// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.os.*;import android.content.*;import android.net.Uri;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;import java.util.*;import java.util.function.Predicate;import org.json.*;

/** Reopen/share-provider checks for a completed installed-app report; no transport opened. */
public final class SimulatorSavedReportInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private AccessibilityNodeInfo find(Predicate<String> match){AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();if(root==null)return null;ArrayDeque<AccessibilityNodeInfo> queue=new ArrayDeque<>();queue.add(root);while(!queue.isEmpty()){AccessibilityNodeInfo node=queue.remove();if(node.getText()!=null&&match.test(node.getText().toString()))return node;for(int i=0;i<node.getChildCount();i++){AccessibilityNodeInfo child=node.getChild(i);if(child!=null)queue.add(child);}}return null;}
 private AccessibilityNodeInfo waitFor(Predicate<String> match)throws Exception{long limit=SystemClock.elapsedRealtime()+8000;AccessibilityNodeInfo node;while((node=find(match))==null&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(100);check(node!=null,"Saved-report UI did not appear");return node;}
 private int probes(Context c){File[] files=new File(c.getFilesDir(),"simulator-hscan-tests").listFiles();return files==null?0:files.length;}
 private String stage="initial";
 public void onStart(){Bundle out=new Bundle();int code=-1;Activity activity=null;
  try{
   Context c=getTargetContext();File dir=new File(c.getFilesDir(),"dtc-reports");File[] candidates=dir.listFiles((d,n)->n.startsWith("hscan_ecm_")&&n.endsWith(".json"));check(candidates!=null,"No saved reports");
   Arrays.sort(candidates,Comparator.comparingLong(File::lastModified).reversed());JSONObject report=null;File file=null;
   for(File f:candidates){JSONObject v=new JSONObject(new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8));if(v.optString("app_version").contains("preview.37")&&v.getJSONObject("result").optString("mode").equals("ecm_dtc")&&!v.optString("transport").equals("synthetic-fixture")){report=v;file=f;break;}}
   check(report!=null,"A completed preview37 ECM read is required");String id=file.getName().replace(".json","");String title="Engine codes — HS-CAN · "+report.getString("observed_utc");int before=probes(c);
   c.getSharedPreferences("updates",0).edit().putBoolean("automatic",false).commit();
   activity=startActivitySync(new Intent().setClassName(c,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();final Activity a=activity;runOnMainSync(()->DtcReportView.showSavedReports(a));
   stage="report list";AccessibilityNodeInfo row=waitFor(text->text.equals(title));for(int i=0;i<4&&!row.isClickable()&&row.getParent()!=null;i++)row=row.getParent();check(row.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Saved report did not open");
   stage="report body";AccessibilityNodeInfo body=waitFor(text->text.contains("OpenSAAB saved diagnostic report"));String contents=body.getText().toString();
   for(String field:new String[]{"Saved observation","Complete ECM current/history","Read at (UTC)","No codes cleared","ECM-only coverage"})check(contents.contains(field),"Report metadata missing: "+field);
   stage="share action";waitFor(text->text.equalsIgnoreCase("Share / email"));check(probes(c)==before&&!SimulatorVehicleConnection.active(),"Reading history opened a connection");
   Uri uri=new Uri.Builder().scheme("content").authority(c.getPackageName()+".dtc-reports").appendPath(id+".txt").build();
   try(InputStream input=c.getContentResolver().openInputStream(uri);ByteArrayOutputStream bytes=new ByteArrayOutputStream()){byte[] chunk=new byte[4096];int n;while((n=input.read(chunk))!=-1)bytes.write(chunk,0,n);check(contents.equals(bytes.toString("UTF-8")),"Share provider returned another report");}
   File screenshot=new File(c.getExternalFilesDir(null),"hscan-alignment/android-saved-hscan-report-after-relaunch.png");screenshot.getParentFile().mkdirs();try(FileOutputStream image=new FileOutputStream(screenshot)){getUiAutomation().takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG,100,image);}
   out.putString("stream","PASS: completed report reopened in native UI after process restart, metadata/raw status retained, exact share-provider contents, no vehicle request\n");
  }catch(Throwable failure){code=0;StringBuilder texts=new StringBuilder();find(text->{if(texts.length()<6000)texts.append(text).append("\n");return false;});out.putString("stream","FAIL: "+stage+": "+failure+"\nUI: "+texts+"\n");}
  finally{if(activity!=null){final Activity a=activity;runOnMainSync(a::finish);}finish(code,out);}
 }
}
