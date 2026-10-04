// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.view.accessibility.AccessibilityNodeInfo;import android.widget.*;
import java.io.*;import java.nio.file.Files;import org.json.*;
/** Real app workflow. Live mode selects the intended M4 bench once; never retries VIN. */
public final class SimulatorVinAlignmentInstrumentedTest extends Instrumentation {
 private boolean live;
 public void onCreate(Bundle b){super.onCreate(b);live="1".equals(b.getString("live"));start();}
 private void check(boolean yes,String message){if(!yes)throw new AssertionError(message);}
 private void main(Runnable task){Throwable[] failure={null};runOnMainSync(()->{try{task.run();}catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
 private void click(String text){AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();if(root!=null)for(AccessibilityNodeInfo n:root.findAccessibilityNodeInfosByText(text))if(n.isClickable()&&n.performAction(AccessibilityNodeInfo.ACTION_CLICK))return;throw new AssertionError("Missing visible control: "+text);}
 private boolean visible(String text){AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();return root!=null&&!root.findAccessibilityNodeInfosByText(text).isEmpty();}
 private void screenshot(String name)throws Exception{waitForIdleSync();SystemClock.sleep(1500);File directory=new File(getTargetContext().getExternalFilesDir(null),"vin-alignment");directory.mkdirs();try(FileOutputStream out=new FileOutputStream(new File(directory,name+".png"))){getUiAutomation().takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG,100,out);}}
 private int attempts(){File[] runs=new File(getTargetContext().getFilesDir(),"simulator-vlinker-tests").listFiles();return runs==null?0:runs.length;}
 private String summary(Activity a){return ((TextView)a.getWindow().getDecorView().findViewWithTag("session-summary")).getText().toString();}
 public void onStart(){Bundle out=new Bundle();com.opensaab.tech2.MainActivity a=null;int code=-1;
  try{
   Context c=getTargetContext();check(SimulatorVehicleConnection.available(c),"Use the Debug ARM64 emulator build with the shared connection core");
   c.getSharedPreferences("updates",0).edit().putBoolean("automatic",false).commit();
   a=(com.opensaab.tech2.MainActivity)startActivitySync(new Intent().setClassName(c,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
   if(visible("Not now")){click("Not now");waitForIdleSync();}
   check(new FirmwareStore(c.getFilesDir()).missing().isEmpty(),"Provision the same private firmware as iOS first");
   final com.opensaab.tech2.MainActivity activity=a;int before=attempts();
   main(()->{
    View root=activity.getWindow().getDecorView();View connect=root.findViewWithTag("connect-start"),offline=root.findViewWithTag("start-offline");
    check(connect.isEnabled()&&offline.isEnabled(),"Ready controls disabled");check(offline.getTop()>connect.getTop(),"Connection controls are not stacked");connect.performClick();
    check(activity.adapterPicker!=null&&activity.adapterPicker.isShowing(),"Select adapter missing");
    activity.adapterPicker.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
   });waitForIdleSync();check(attempts()==before,"Picker cancellation opened Bluetooth");screenshot("android-ready-aligned");
   if(live){
    main(()->{activity.getWindow().getDecorView().findViewWithTag("connect-start").performClick();ListView list=activity.adapterPicker.getListView();check(list.getAdapter().getItem(0).toString().equals("Carista EVO / vLinker MC+ · Bluetooth"),"Wrong adapter selection");list.performItemClick(list.getChildAt(0),0,list.getAdapter().getItemId(0));});
    long deadline=SystemClock.elapsedRealtime()+45000;
    while(SystemClock.elapsedRealtime()<deadline){final boolean[] done={false};main(()->done[0]=summary(activity).contains("VIN: YS3")&&activity.getWindow().getDecorView().findViewWithTag("connect-start").isEnabled());if(done[0])break;SystemClock.sleep(100);}
    main(()->{
     String text=summary(activity);check(text.contains("VIN: YS3"),"Fresh VIN missing: "+text);check(text.contains("V"),"Observed adapter voltage missing: "+text);
     check(((TextView)activity.getWindow().getDecorView().findViewWithTag("connection-status")).getText().toString().contains("firmware transport is not implemented"),"Missing firmware limitation");
    });waitForIdleSync();screenshot("android-fresh-vin-after-release");
    check(attempts()==before+1,"Live workflow attempted more than one discovery");
    File[] runs=new File(c.getFilesDir(),"simulator-vlinker-tests").listFiles();java.util.Arrays.sort(runs,java.util.Comparator.comparingLong(File::lastModified));
    JSONObject report=new JSONObject(new String(Files.readAllBytes(new File(runs[runs.length-1],"result.json").toPath()),"UTF-8"));
    check(report.getString("status").equals("vin_received")&&report.getBoolean("bluetooth_closed"),"VIN was shown before release");
    check(report.getInt("host_vehicle_request_attempts")==1&&report.getInt("host_vehicle_requests_written")==1&&!report.getBoolean("automatic_replay"),"VIN request was replayed");
    String vin=report.getJSONObject("result").getString("vin");main(()->check(summary(activity).contains(vin),"Displayed VIN differs from raw report"));
    main(()->activity.getWindow().getDecorView().findViewWithTag("session-summary").performClick());waitForIdleSync();check(visible(vin),"Current VIN missing from details");check(visible("GEARBOX, FA57")&&visible("Sedan / Saloon"),"Descriptive fields missing from details");screenshot("android-current-vehicle-details");click("Done");waitForIdleSync();
    main(()->activity.getWindow().getDecorView().findViewWithTag("start-offline").performClick());SystemClock.sleep(500);main(()->check(!summary(activity).contains(vin),"Offline inherited current VIN"));screenshot("android-offline-clears-current-vin");
    main(()->activity.getWindow().getDecorView().findViewWithTag("session-summary").performClick());waitForIdleSync();check(!visible(vin)&&(visible("No current vehicle connection")||visible("Last vehicle")),"Offline retained current vehicle details");click("Done");waitForIdleSync();
   }
   out.putString("stream","PASS: aligned stacked workspace, picker cancellation opens no transport"+(live?", one real VIN, confirmed Bluetooth release, matching 2004/B207R metadata, explicit firmware gap, current details and offline identity clearing":"; no live request")+"\n");
  }catch(Throwable failure){code=0;out.putString("stream","FAIL: "+failure+"\n");try{screenshot("android-failure");}catch(Exception ignored){}}
  finally{if(a!=null){final Activity activity=a;main(activity::finish);}finish(code,out);}
 }
}
