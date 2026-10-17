package com.opensaab.usb;
import android.app.*;import android.os.*;import android.content.*;
public final class SecurityOpenAccessInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 static void check(boolean value,String note){if(!value)throw new AssertionError(note);}
 public void onStart(){Bundle result=new Bundle();Activity activity=null;
  try{
   activity=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity host=activity;waitForIdleSync();
   check(SecurityAuthorization.available(host),"feature access does not need a saved operator credential");
   check("OpenSAAB".equals(SecurityAuthorization.bearer(host)),"public primary-service compatibility header remains available; it is not caller authentication");
   final boolean[] continued={false};runOnMainSync(()->SecurityAuthorization.show(host,()->continued[0]=true));waitForIdleSync();
   check(continued[0],"processing continuation requires no password dialog");
   android.view.accessibility.AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();
   check(root==null||root.findAccessibilityNodeInfosByText("Security access password").isEmpty(),"no operator password dialog");
   result.putString("stream","PASS: exact product binding continues without password entry or saved credential; public primary-service compatibility header retained (not caller authentication); no network, adapter or import operation\n");finish(Activity.RESULT_OK,result);
  }catch(Throwable e){result.putString("stream","FAIL: "+e);finish(Activity.RESULT_CANCELED,result);}
  finally{if(activity!=null){final Activity host=activity;runOnMainSync(host::finish);}}
 }
}
