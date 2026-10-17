package com.opensaab.usb;
import android.app.*;import android.os.*;import android.content.*;import android.view.KeyEvent;import android.widget.TextView;
public final class SecurityPreviewInstrumentedTest extends Instrumentation {
 public static class Host extends Activity {public void onCreate(Bundle b){super.onCreate(b);TextView t=new TextView(this);t.setText("Security preview policy test");setContentView(t);}}
 boolean preview;
 public void onCreate(Bundle b){preview="true".equals(b.getString("preview"));start();}
 static void check(boolean v,String n){if(!v)throw new AssertionError(n);}
 public void onStart(){Bundle result=new Bundle();Activity activity=null;
  try{
   activity=startActivitySync(new Intent(getTargetContext(),Host.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity host=activity;
   check(SecurityAuthorization.previewNoPrompt(host)==preview,"packaged channel selection");
   check(SecurityAuthorization.available(host)==preview,"preview requires no saved credential");
   check(!new java.io.File(host.getNoBackupFilesDir(),"security-authorization.enc").exists(),"test starts with no saved password");
   if(preview)check(SecurityAuthorization.bearer(host).length()>0,"internal service authentication retained");
   else {boolean blocked=false;try{SecurityAuthorization.bearer(host);}catch(java.io.IOException expected){blocked=true;}check(blocked,"qualified build still requires password");}
   final boolean[] called={false};final Throwable[] error={null};
   runOnMainSync(()->{try{SecurityAuthorization.show(host,()->called[0]=true);}catch(Throwable t){error[0]=t;}});waitForIdleSync();
   check(error[0]==null,"password settings call");
   if(preview)check(called[0],"preview continues without password dialog");
   else {check(!called[0],"qualified build does not bypass dialog");boolean visible=false;for(int i=0;i<30&&!visible;i++){android.view.accessibility.AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();visible=root!=null&&!root.findAccessibilityNodeInfosByText("Security access password").isEmpty();if(!visible)Thread.sleep(100);}check(visible,"qualified password dialog remains visible");sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);}
   runOnMainSync(host::finish);result.putString("stream","POLICY PASS preview="+preview);finish(Activity.RESULT_OK,result);
  }catch(Throwable e){result.putString("stream","POLICY FAIL "+e);finish(Activity.RESULT_CANCELED,result);}
 }
}
