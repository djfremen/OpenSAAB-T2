// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;import java.util.concurrent.atomic.AtomicBoolean;

/** Idle production chooser only, with a callback recorder; opens no adapter. */
public final class SessionModePickerInstrumentedTest extends Instrumentation {
 private Bundle arguments;
 public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
 private static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
 private void ui(Runnable action){final Throwable[] error={null};runOnMainSync(()->{try{action.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
 public void onStart(){Bundle result=new Bundle();Activity host=null;try{
  DisposableEmulator.require(getTargetContext());DisposableEmulator.requireInstalledVersion(getTargetContext(),arguments);
  host=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity activity=host;
  waitForIdleSync();ui(()->{View continueButton=activity.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(continueButton!=null)continueButton.performClick();});
  final int[] calls={0};final SessionModes.Mode[] chosen={null};AtomicBoolean ready=new AtomicBoolean(true);
  for(String family:new String[]{"chipsoft","mxplus","nano","mdi","bluetooth"}){
   SessionModes.Capabilities caps=SessionModes.forAdapter(family,false);final AlertDialog[] picker={null};
   ui(()->picker[0]=SessionModePicker.show(activity,caps,ready::get,mode->{calls[0]++;chosen[0]=mode;}));
   waitForIdleSync();final int before=calls[0];
   ui(()->{View root=picker[0].getWindow().getDecorView();RadioButton read=root.findViewWithTag("session-mode-read"),write=root.findViewWithTag("session-mode-write");Button connect=picker[0].getButton(-1);
    check(read.isEnabled()==caps.read,"Wrong read availability: "+family);check(write.isEnabled()==(caps.write!=null),"Wrong write availability: "+family);
    check(read.isChecked()==caps.read,"Read must be the initial choice only where enforced");check(connect.isEnabled()==caps.read,"No implicit full-control default");
    if(caps.write!=null)write.performClick();picker[0].getButton(-2).performClick();check(calls[0]==before,"Cancel started a session");
   });
   if(caps.write!=null){
    ui(()->picker[0]=SessionModePicker.show(activity,caps,ready::get,mode->{calls[0]++;chosen[0]=mode;}));waitForIdleSync();
    ui(()->{((RadioButton)picker[0].getWindow().getDecorView().findViewWithTag("session-mode-write")).performClick();ready.set(false);picker[0].getButton(-1).performClick();check(calls[0]==before,"Background/stale choice started a session");ready.set(true);picker[0].getButton(-1).performClick();picker[0].getButton(-1).performClick();check(calls[0]==before+1&&chosen[0]==caps.write,"Wrong or replayed mode callback");});
   }
   if(caps.read){final int readBefore=calls[0];
    ui(()->picker[0]=SessionModePicker.show(activity,caps,ready::get,mode->{calls[0]++;chosen[0]=mode;}));waitForIdleSync();
    ui(()->{picker[0].getButton(-1).performClick();check(calls[0]==readBefore+1&&chosen[0]==SessionModes.Mode.READ,"Default read selection changed authority");});
   }
  }
  result.putString("stream","PASS: same USB/Bluetooth production chooser; enforced read defaults, explicit MDI write choice, limited MX+/Nano clearing labels, unsupported modes disabled, Cancel/stale/once-only selection; zero adapter or firmware operations");finish(-1,result);
 }catch(Throwable error){result.putString("stream","FAIL: "+error);finish(0,result);}finally{if(host!=null){final Activity activity=host;runOnMainSync(activity::finish);}}}
}
