// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;
import android.app.*;import android.content.*;import android.content.pm.*;import android.os.*;import android.view.*;import android.widget.*;import java.lang.reflect.*;
/** Disposable emulator only. Native uninstall is intercepted/cancelled; no user data removed. */
public final class SetupCleanupInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 static void check(boolean b,String s){if(!b)throw new AssertionError(s);}
 static Object field(Object a,String n)throws Exception{Field f=MainActivity.class.getDeclaredField(n);f.setAccessible(true);return f.get(a);}
 public void onStart(){Bundle result=new Bundle();int code=Activity.RESULT_OK;MainActivity activity=null;ActivityMonitor monitor=null;
 try{
  Context c=getTargetContext();PackageInfo installed=c.getPackageManager().getPackageInfo("com.opensaab.tech2",PackageManager.GET_SIGNATURES);
  check(MainActivity.removalEligible(installed,new Intent()),"Signed app should allow cleanup");
  check(!MainActivity.removalEligible(null,new Intent()),"Absent app must not allow cleanup");
  check(!MainActivity.removalEligible(new PackageInfo(),new Intent()),"Unverified app must not allow cleanup");
  check(!MainActivity.removalEligible(installed,null),"Unlaunchable app must not allow cleanup");
  Intent self=MainActivity.selfRemovalIntent(c);check(self.getAction().equals(Intent.ACTION_UNINSTALL_PACKAGE)&&self.getData().toString().equals("package:com.opensaab.checker"),"Must remove only Setup");
  c.getSharedPreferences("setup-completion",0).edit().clear().commit();
  activity=(MainActivity)startActivitySync(new Intent(c,MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final MainActivity a=activity;
  long end=SystemClock.elapsedRealtime()+20000;
  while(field(a,"cleanupDialog")==null&&SystemClock.elapsedRealtime()<end)SystemClock.sleep(100);
  check(field(a,"cleanupDialog")!=null,"Completed installation should offer cleanup");
  runOnMainSync(()->{try{((AlertDialog)field(a,"cleanupDialog")).getButton(AlertDialog.BUTTON_NEGATIVE).performClick();}catch(Exception e){throw new RuntimeException(e);}});waitForIdleSync();
  runOnMainSync(()->{try{Method update=MainActivity.class.getDeclaredMethod("update");update.setAccessible(true);update.invoke(a);}catch(Exception e){throw new RuntimeException(e);}});waitForIdleSync();
  check(field(a,"cleanupDialog")==null,"Keeping Setup must not nag again");
  check(((Button)field(a,"removeSetup")).getVisibility()==View.VISIBLE,"Later removal remains available");
  IntentFilter filter=new IntentFilter(Intent.ACTION_UNINSTALL_PACKAGE);filter.addDataScheme("package");
  monitor=addMonitor(filter,new ActivityResult(Activity.RESULT_CANCELED,null),true);
  runOnMainSync(()->{try{((Button)field(a,"removeSetup")).performClick();((AlertDialog)field(a,"cleanupDialog")).getButton(AlertDialog.BUTTON_POSITIVE).performClick();}catch(Exception e){throw new RuntimeException(e);}});waitForIdleSync();
  check(monitor.getHits()==1,"Expected user-confirmed system removal flow");
  check(c.getPackageManager().getPackageInfo("com.opensaab.tech2",PackageManager.GET_SIGNATURES).versionCode==installed.versionCode,"Emulator must remain installed");
  check(c.getPackageManager().getPackageInfo("com.opensaab.checker",0)!=null,"Cancelled removal must retain Setup");
  result.putString("stream","PASS: absence/signature/launch gates, self-only uninstall target, automatic completion offer, keep/no-repeat, later removal, cancellation and emulator preservation");
 }catch(Throwable e){code=Activity.RESULT_CANCELED;result.putString("stream","FAIL: "+e);}
 finally{if(monitor!=null)removeMonitor(monitor);if(activity!=null){MainActivity a=activity;runOnMainSync(a::finish);}}
 finish(code,result);
 }
}
