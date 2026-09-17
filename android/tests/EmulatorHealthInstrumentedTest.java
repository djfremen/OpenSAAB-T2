// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.*;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.io.File;

/** Synthetic health faults only: no adapter, API, or firmware writes. */
public final class EmulatorHealthInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 static void check(boolean b,String why){if(!b)throw new AssertionError(why);}
 static TextView text(View v,String label){if(v instanceof TextView&&label.contentEquals(((TextView)v).getText()))return (TextView)v;if(v instanceof ViewGroup)for(int i=0;i<((ViewGroup)v).getChildCount();i++){TextView f=text(((ViewGroup)v).getChildAt(i),label);if(f!=null)return f;}return null;}
 public void onStart(){Bundle out=new Bundle();Activity a=null;EmulatorHealthMonitor monitor=null;try{
  EmulatorHealthState s=new EmulatorHealthState();s.reset(0);
  for(long t=1000;t<=240000;t+=1000){s.heartbeat(t);s.ui(t);check(s.reason(t)==null,"Idle static screen flagged");}
  s.input(240000);s.input(250000);s.heartbeat(270000);s.ui(270000);check("input_without_display_response".equals(s.reason(270000)),"Repeated keys postponed detection");
  s.frame();check(s.reason(270000)==null,"Frame did not resolve pending input");
  s.ui(300000);check("emulator_heartbeat_missing".equals(s.reason(300000)),"Dead loop missed");
  s.reset(0);s.ui(179999);check(s.reason(179999)==null,"Slow boot flagged early");s.ui(180000);check("emulator_heartbeat_missing".equals(s.reason(180000)),"No startup heartbeat missed");
  s.reset(0);s.heartbeat(20000);check("ui_unresponsive".equals(s.reason(20000)),"Blocked UI missed");s.alerted=true;check(s.reason(1000000)==null,"Repeated alert");
  a=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity activity=a;
  final EmulatorHealthMonitor[] holder={null};final boolean[] stopped={false};
  runOnMainSync(()->holder[0]=new EmulatorHealthMonitor(activity,()->stopped[0]=true));monitor=holder[0];
  File run=new File(activity.getCacheDir(),"health-synthetic");run.mkdirs();monitor.begin(run);monitor.ended();waitForIdleSync();
  java.lang.reflect.Field field=EmulatorHealthMonitor.class.getDeclaredField("dialog");field.setAccessible(true);AlertDialog dialog=(AlertDialog)field.get(monitor);
  check(dialog!=null&&dialog.isShowing(),"No report prompt on unexpected exit");
  check(text(dialog.getWindow().getDecorView(),"Send report to OpenSAAB")!=null,"Missing report action");
  check(!stopped[0],"Detection stopped emulator automatically");
  runOnMainSync(()->dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick());
  check(!stopped[0],"Keep waiting stopped emulator");
  final EmulatorHealthMonitor first=monitor;runOnMainSync(first::close);final EmulatorHealthMonitor[] next={null};runOnMainSync(()->next[0]=new EmulatorHealthMonitor(activity,()->{}));monitor=next[0];monitor.begin(run);monitor.expectedStop();monitor.ended();waitForIdleSync();check(field.get(monitor)==null,"User stop flagged as crash");
  org.json.JSONObject report=SupportReports.collect(activity,"Synthetic health test");check(report.has("emulator_health"),"Health evidence missing from report");
  monitor.begin(run);monitor.ended();waitForIdleSync();
  AlertDialog reportDialog=(AlertDialog)field.get(monitor);
  ActivityMonitor reportActivity=addMonitor("com.opensaab.usb.SupportReportActivity",null,false);
  runOnMainSync(()->reportDialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick());
  Activity reportScreen=waitForMonitorWithTimeout(reportActivity,5000);check(reportScreen!=null,"Report action did not open review flow");
  check(text(reportScreen.getWindow().getDecorView(),"Prepare report")!=null,"Report review skipped");
  runOnMainSync(reportScreen::finish);removeMonitor(reportActivity);
  new File(activity.getFilesDir(),"last-emulator-health.json").delete();run.delete();
  out.putString("stream","PASS: static screens, slow boot grace, loop stall, input stall, UI stall, one prompt, expected stop, report evidence; no uploads or vehicle commands\n");finish(-1,out);
 }catch(Throwable e){out.putString("stream","FAIL: "+e+"\n");finish(0,out);}finally{final EmulatorHealthMonitor m=monitor;final Activity end=a;runOnMainSync(()->{if(m!=null)m.close();if(end!=null)end.finish();});}}
}
