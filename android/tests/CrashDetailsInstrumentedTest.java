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
    out.putString("stream","PASS: packaged native summary, rejected model/firmware/hardware, unknown driver, linked connection, independent USB cleanup, terminal PC/exit, identical saved snapshot, original failure preserved, serial excluded\n");
  }catch(Throwable e){result=0;out.putString("stream","FAIL: "+e+"\n");}finish(result,out);}
}
