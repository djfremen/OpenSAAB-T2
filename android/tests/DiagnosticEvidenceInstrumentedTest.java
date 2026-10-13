// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;import android.os.Bundle;import java.io.*;import java.util.*;import org.json.*;
public final class DiagnosticEvidenceInstrumentedTest extends Instrumentation {
  public void onCreate(Bundle b){super.onCreate(b);start();}
  void check(boolean b,String m){if(!b)throw new AssertionError(m);}
  public void onStart(){Bundle out=new Bundle();int code=-1;try{
    File run=new File(getTargetContext().getFilesDir(),"chipsoft-"+UUID.randomUUID());run.mkdir();
    ConnectionAttempt attempt=new ConnectionAttempt(getTargetContext(),ConnectionAttempt.Adapter.CHIPSOFT);
    attempt.bind(run,"read_only","vin");attempt.stage(ConnectionAttempt.Stage.VIN_REQUEST);
    attempt.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.PROTOCOL_OR_PROCESS_ERROR);
    attempt.cleanup(1,false,"succeeded","unknown");attempt.finish(ConnectionAttempt.Outcome.FAILED,ConnectionAttempt.Reason.CLEANUP_FAILED);
    ConnectionAttempt.collect(getTargetContext()); // flush asynchronous evidence writes
    JSONObject e=DiagnosticEvidence.collect(run,"chipsoft");
    check(e.getString("primary_reason").equals("PROTOCOL_OR_PROCESS_ERROR"),"primary error overwritten");
    check(e.getJSONObject("cleanup").getString("usb_release").equals("succeeded"),"process failure conflated with USB cleanup");
    check(e.getJSONObject("cleanup").getInt("child_exit_code")==1,"child exit omitted");
    check(e.getJSONObject("artifacts").getString("crash.json").equals("missing"),"missing evidence hidden");
    JSONArray pcs=new JSONArray();for(int i=0;i<80;i++)pcs.put(i);
    FirmwareStore.writeJson(new File(run,"crash.json"),new JSONObject().put("pc",94662).put("recent_pcs",pcs).put("stack_bytes","SECRET").put("screen","SECRET"));
    e=DiagnosticEvidence.collect(run,"nano");check(e.getString("adapter").equals("nano"),"Nano mislabeled");
    check(e.getJSONObject("terminal_fault").getJSONArray("recent_pcs").length()==32,"PC list unbounded");check(!e.toString().contains("SECRET"),"private memory leaked");
    try(FileWriter w=new FileWriter(new File(run,"crash.json"))){w.write("broken");}
    check(DiagnosticEvidence.collect(run,"unknown").getJSONObject("artifacts").getString("crash.json").equals("invalid"),"invalid evidence hidden");
    out.putString("stream","PASS: schema2 adapter identity, primary failure preserved, independent child/USB/protocol outcomes, missing/corrupt evidence, bounded terminal PCs, memory privacy\n");
  }catch(Throwable e){code=0;out.putString("stream","FAIL: "+e.toString()+"\n");}finish(code,out);}
}
