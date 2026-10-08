// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.Manifest;import android.app.*;import android.content.*;import android.content.pm.PackageManager;import android.os.*;import android.view.*;import android.view.accessibility.AccessibilityNodeInfo;import android.widget.*;
import java.io.*;import java.lang.reflect.*;
/** Installed picker permission/UI checks on an emulator. No adapter selected, no vehicle I/O. */
public final class BluetoothPermissionInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle args){super.onCreate(args);start();}
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private void main(Runnable r){Throwable[] failure={null};runOnMainSync(()->{try{r.run();}catch(Throwable t){failure[0]=t;}});if(failure[0]!=null)throw new AssertionError(failure[0]);}
 private boolean click(String text){AccessibilityNodeInfo root=getUiAutomation().getRootInActiveWindow();if(root!=null)for(AccessibilityNodeInfo n:root.findAccessibilityNodeInfosByText(text))if(n.isClickable()&&n.performAction(AccessibilityNodeInfo.ACTION_CLICK))return true;return false;}
 private void clickWait(String text){long until=SystemClock.elapsedRealtime()+5000;while(SystemClock.elapsedRealtime()<until){if(click(text))return;SystemClock.sleep(100);}throw new AssertionError("Missing permission control: "+text);}
 private Dialog dialog(VlinkerDevicePicker picker)throws Exception{Field f=VlinkerDevicePicker.class.getDeclaredField("dialog");f.setAccessible(true);return (Dialog)f.get(picker);}
 private void shell(String cmd)throws Exception{try(ParcelFileDescriptor fd=getUiAutomation().executeShellCommand(cmd);FileInputStream input=new FileInputStream(fd.getFileDescriptor())){while(input.read()!=-1){}}}
 public void onStart(){Bundle out=new Bundle();int code=-1;com.opensaab.tech2.MainActivity activity=null;VlinkerDevicePicker picker=null;
  try{
   Context c=getTargetContext();check(Build.VERSION.SDK_INT>=31&&(Build.HARDWARE.equals("ranchu")||Build.HARDWARE.equals("goldfish"))&&VlinkerVehicleConnection.available(c),"API31+ emulator with shared core required");
   check(c.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)!=PackageManager.PERMISSION_GRANTED,"Begin with Nearby devices revoked");
   c.getSharedPreferences("updates",0).edit().putBoolean("automatic",false).commit();activity=(com.opensaab.tech2.MainActivity)startActivitySync(new Intent().setClassName(c,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
   final com.opensaab.tech2.MainActivity a=activity;final VlinkerDevicePicker[] selected={null};
   main(()->{try{
    Field f=a.getClass().getDeclaredField("bluetoothPicker");f.setAccessible(true);
    if(SimulatorVehicleConnection.available(c)){selected[0]=new VlinkerDevicePicker(a,(device,compatibility)->{throw new AssertionError("Permission test selected hardware");});f.set(a,selected[0]);selected[0].show();}
    else{a.getWindow().getDecorView().findViewWithTag("connect-start").performClick();ListView list=a.adapterPicker.getListView();check(list.getAdapter().getItem(0).toString().equals("Carista EVO / vLinker MC+ · Bluetooth"),"Release adapter entry missing");list.performItemClick(list.getChildAt(0),0,list.getAdapter().getItemId(0));selected[0]=(VlinkerDevicePicker)f.get(a);}
    check(selected[0]!=null&&!dialog(selected[0]).findViewById(android.R.id.content).findViewWithTag("vlinker-use-selected").isEnabled(),"Permission gate enabled connect");
   }catch(Exception e){throw new AssertionError(e);}});waitForIdleSync();final VlinkerDevicePicker p=selected[0];picker=p;
   check(VlinkerDevicePicker.androidName(" vLinker MC-Android ")&&VlinkerDevicePicker.androidName("vLinker MC")&&VlinkerDevicePicker.androidName("Carista EVO")&&!VlinkerDevicePicker.androidName("Carista OBD")&&!VlinkerDevicePicker.androidName("Carista EVOLUTION")&&!VlinkerDevicePicker.androidName("vLinker MC-IOS"),"Android profile filter wrong");
   clickWait("Turn on Bluetooth");clickWait("Don’t allow");waitForIdleSync();SystemClock.sleep(200);
   main(()->{try{check(dialog(p).isShowing(),"Denial dismissed picker");check(!dialog(p).findViewById(android.R.id.content).findViewWithTag("vlinker-use-selected").isEnabled(),"Denied permission enabled connect");}catch(Exception e){throw new AssertionError(e);}});
   check(!VlinkerVehicleConnection.active(),"Permission denial opened a connection");
   clickWait("Refresh paired adapters");clickWait("Allow");waitForIdleSync();SystemClock.sleep(300);
   check(c.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)==PackageManager.PERMISSION_GRANTED,"Grant callback failed");
   main(()->{try{p.refresh();View root=dialog(p).findViewById(android.R.id.content);check(!root.findViewWithTag("vlinker-use-selected").isEnabled(),"Missing adapter enabled connect");check(!((CheckBox)findCheck(root)).isChecked(),"Unauthenticated SPP selected by default");}catch(Exception e){throw new AssertionError(e);}});
   check(!VlinkerVehicleConnection.active(),"Refresh opened a connection");
   File dir=new File(c.getExternalFilesDir(null),"direct-bluetooth");dir.mkdirs();try(FileOutputStream image=new FileOutputStream(new File(dir,"android-bluetooth-picker.png"))){getUiAutomation().takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG,100,image);}
   main(p::close);out.putString("stream","PASS: installed API36 picker (release uses normal Connect/adapter selection), Nearby devices denial/retry/grant callback, no adapter means no connect, Android name filtering, secure SPP default, no automatic vehicle access\n");
  }catch(Throwable error){code=0;out.putString("stream","FAIL: "+error+"\n");}
  finally{if(picker!=null){final VlinkerDevicePicker p=picker;main(p::close);}if(activity!=null){final Activity a=activity;main(a::finish);}finish(code,out);}
 }
 private View findCheck(View view){if(view instanceof CheckBox)return view;if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){View value=findCheck(group.getChildAt(i));if(value!=null)return value;}}return null;}
}
