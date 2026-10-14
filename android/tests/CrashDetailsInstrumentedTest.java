// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.os.Bundle;
import java.io.File;
import java.util.UUID;
import org.json.JSONObject;
/** Synthetic evidence only. No firmware execution, USB access or uploads. */
public final class CrashDetailsInstrumentedTest extends Instrumentation {
  public void onCreate(Bundle b){super.onCreate(b);start();}
  void check(boolean v,String why){if(!v)throw new AssertionError(why);}
  public void onStart(){Bundle out=new Bundle();int result=-1;try{
    File run=new File(getTargetContext().getFilesDir(),"chipsoft-"+UUID.randomUUID());run.mkdir();
    ConnectionAttempt attempt=new ConnectionAttempt(getTargetContext(),ConnectionAttempt.Adapter.CHIPSOFT);
    attempt.bind(run,"read_only","original_menus");
    attempt.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.PROTOCOL_OR_PROCESS_ERROR);
    attempt.cleanup(3,false,"succeeded","unknown");
    String reason="CANdi initialization failed: Unvalidated Chipsoft firmware version";
    FirmwareStore.writeJson(new File(run,"adapter-diagnostics.json"),new JSONObject()
      .put("schema",1).put("family","chipsoft").put("model","Chipsoft J2534 Mid")
      .put("firmware_version","1.5.2").put("hardware_version","00000003")
      .put("transport","android_usb").put("compatibility","rejected")
      .put("state","initialization_failed").put("stage","identification")
      .put("usb_release","unknown").put("protocol_cleanup","unknown").put("serial","PRIVATE_SERIAL"));
    FirmwareStore.writeJson(new File(run,"candi-startup.json"),new JSONObject().put("status","failed").put("error",reason));
    FirmwareStore.writeJson(new File(run,"crash.json"),new JSONObject().put("reason",reason).put("pc",94486));
    FirmwareStore.writeJson(new File(run,"report.json"),new JSONObject().put("reason",reason).put("pc",94486).put("exit_code",3));
    String text=ConnectionAttempt.failureSummary(getTargetContext(),run);
    check(text.startsWith("CRASH DETAILS"),"native formatter failed: "+text);
    check(text.contains("Chipsoft J2534 Mid")&&text.contains("Firmware: 1.5.2"),"rejected identity lost");
    check(text.contains("hardware 00000003")&&text.contains("driver unavailable"),"version evidence wrong");
    check(text.contains("state initialization_failed")&&text.contains("USB succeeded"),"failure and USB cleanup conflated");
    check(text.contains("adapter_firmware_not_validated")&&!text.contains("PRIVATE_SERIAL"),"failure category/privacy wrong");
    JSONObject snapshot=FirmwareStore.json(new File(run,"crash-diagnostics.json"));
    JSONObject crash=FirmwareStore.json(new File(run,"crash.json")),report=FirmwareStore.json(new File(run,"report.json"));
    check(crash.getString("reason").equals(reason)&&report.getString("reason").equals(reason),"first failure overwritten");
    check(crash.getJSONObject("crash_diagnostics").toString().equals(snapshot.toString())&&report.getJSONObject("crash_diagnostics").toString().equals(snapshot.toString()),"logs disagree");
    check(snapshot.getString("architecture").equals("arm64-v8a")&&!snapshot.isNull("attempt_id"),"package/session metadata lost");
    AdapterCompatibilityNotice.Notice mid=AdapterCompatibilityNotice.read(run);
    check(mid!=null&&mid.code.equals("model_not_validated")&&mid.url.equals(AdapterCompatibilityNotice.INFO),"Mid incorrectly sent to Pro update");
    JSONObject adapter=FirmwareStore.json(new File(run,"adapter-diagnostics.json"));
    adapter.put("model","Chipsoft J2534 Pro").put("firmware_version","1.5.1");
    FirmwareStore.writeJson(new File(run,"adapter-diagnostics.json"),adapter);
    String older=ConnectionAttempt.failureSummary(getTargetContext(),run);
    AdapterCompatibilityNotice.Notice oldPro=AdapterCompatibilityNotice.read(run);
    check(oldPro!=null&&oldPro.code.equals("older_pro_firmware")&&oldPro.url.equals(AdapterCompatibilityNotice.PRO),"older Pro update guidance missing");
    check(older.contains("firmware 1.5.1")&&older.contains("Pro firmware 1.5.2")&&older.contains(AdapterCompatibilityNotice.PRO),"Console guidance incomplete");
    adapter.put("firmware_version","1.5.2");FirmwareStore.writeJson(new File(run,"adapter-diagnostics.json"),adapter);
    ConnectionAttempt.failureSummary(getTargetContext(),run);
    check(AdapterCompatibilityNotice.read(run).code.equals("identity_not_validated"),"same-version rejection called outdated");
    android.app.Activity activity=startActivitySync(new android.content.Intent().setClassName(getTargetContext().getPackageName(),"com.opensaab.tech2.MainActivity").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK));
    final EmulatorHealthMonitor[] monitor=new EmulatorHealthMonitor[1];final boolean[] stopCalled={false};
    runOnMainSync(()->{monitor[0]=new EmulatorHealthMonitor(activity,()->stopCalled[0]=true);monitor[0].begin(run);monitor[0].endedCompatibility(oldPro);});waitForIdleSync();
    java.lang.reflect.Field dialogField=EmulatorHealthMonitor.class.getDeclaredField("dialog");dialogField.setAccessible(true);
    android.app.AlertDialog dialog=(android.app.AlertDialog)dialogField.get(monitor[0]);
    check(dialog!=null&&dialog.isShowing(),"compatibility message not displayed");
    android.widget.TextView message=dialog.findViewById(android.R.id.message);
    check(message.getText().toString().contains("firmware 1.5.1")&&message.getText().toString().contains("firmware 1.5.2"),"dialog lost detected/tested versions");
    check(dialog.getButton(android.app.AlertDialog.BUTTON_NEUTRAL).getText().toString().equals("Pro driver/firmware"),"official link action missing");
    check(!stopCalled[0],"notice automatically restarted/stopped a session");
    JSONObject event=FirmwareStore.json(new File(getTargetContext().getFilesDir(),"last-emulator-health.json"));
    check(event.getString("reason").equals("unexpected_emulator_exit"),"compatibility failure relabelled vehicle-network failure");
    android.graphics.Bitmap screen=getUiAutomation().takeScreenshot();
    if(screen!=null){try(java.io.FileOutputStream image=new java.io.FileOutputStream(new File(getTargetContext().getFilesDir(),"chipsoft-compatibility-test.png"))){screen.compress(android.graphics.Bitmap.CompressFormat.PNG,100,image);}screen.recycle();}
    runOnMainSync(()->{monitor[0].close();activity.finish();});
    out.putString("stream","PASS: native snapshot and cleanup, serial excluded, older Pro vs current Pro vs Mid, official link, real compatibility dialog with detected/tested versions, no automatic restart/update/upload, unchanged failure category\n");
  }catch(Throwable e){result=0;out.putString("stream","FAIL: "+e+"\n");}finish(result,out);}
}
