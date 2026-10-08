// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;import java.io.*;import java.nio.file.Files;import org.json.*;
/** Explicit ECM-only native probe. One selected read, no automatic retry. */
public final class SimulatorHscanInstrumentedTest extends Instrumentation {
 private boolean live;private String mode;
 public void onCreate(Bundle b){super.onCreate(b);live="1".equals(b.getString("live"));mode=b.getString("mode","ecm_info");start();}
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private void main(Runnable r){Throwable[] fail={null};runOnMainSync(()->{try{r.run();}catch(Throwable t){fail[0]=t;}});if(fail[0]!=null)throw new AssertionError(fail[0]);}
 private int attempts(){File[] runs=new File(getTargetContext().getFilesDir(),"simulator-hscan-tests").listFiles();return runs==null?0:runs.length;}
 private void screenshot(String name)throws Exception{waitForIdleSync();SystemClock.sleep(1500);File dir=new File(getTargetContext().getExternalFilesDir(null),"hscan-alignment");dir.mkdirs();try(FileOutputStream out=new FileOutputStream(new File(dir,name+".png"))){getUiAutomation().takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG,100,out);}}
 public void onStart(){Bundle out=new Bundle();int code=-1;com.opensaab.tech2.MainActivity activity=null;
  try{
   Context c=getTargetContext();check(SimulatorVehicleConnection.available(c),"Use Debug M4 emulator with shared connection executable");
   c.getSharedPreferences("updates",0).edit().putBoolean("automatic",false).commit();
   activity=(com.opensaab.tech2.MainActivity)startActivitySync(new Intent().setClassName(c,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));waitForIdleSync();
   final com.opensaab.tech2.MainActivity a=activity;int before=attempts();main(()->check(a.showHscanProbe()!=null,"Probe sheet unavailable"));waitForIdleSync();
   if(live){
    main(()->a.hscanDialog.findViewById(android.R.id.content).findViewWithTag(mode.equals("ecm_dtc")?"hscan-read-codes":"hscan-read-info").performClick());
    long limit=SystemClock.elapsedRealtime()+40000;final boolean[] done={false};
    while(SystemClock.elapsedRealtime()<limit){main(()->done[0]=a.hscanDialog.findViewById(android.R.id.content).findViewWithTag("hscan-read-info").isEnabled());if(done[0])break;SystemClock.sleep(100);}
    check(done[0],"Probe timed out");check(attempts()==before+1,"Probe replayed");
    File[] runs=new File(c.getFilesDir(),"simulator-hscan-tests").listFiles();java.util.Arrays.sort(runs,java.util.Comparator.comparingLong(File::lastModified));File run=runs[runs.length-1];
    JSONObject report=new JSONObject(new String(Files.readAllBytes(new File(run,"result.json").toPath()),"UTF-8"));
    check(report.getString("status").equals("complete"),"ECM probe failed: "+report.optString("error"));check(report.getBoolean("bluetooth_closed"),"BLE release not confirmed");
    check(report.getInt("host_vehicle_requests_written")==1&&report.getInt("host_vehicle_request_attempts")==1&&!report.getBoolean("automatic_replay"),"VIN replay or write not completed");
    JSONObject result=report.getJSONObject("result");check(result.getString("vin").startsWith("YS3")&&result.getString("mode").equals(mode),"Wrong identity/scope");
    if(mode.equals("ecm_dtc")){JSONObject dtc=result.getJSONObject("dtc_report");check(dtc.getBoolean("complete")&&!dtc.getBoolean("codes_cleared")&&result.getBoolean("diagnostic_session_stopped"),"Incomplete DTC or session cleanup");}
    else check(result.getJSONObject("ecm_identifiers").length()==7,"Incomplete identifier suite");
    String trace=new String(Files.readAllBytes(new File(run,"transcript.jsonl").toPath()),"UTF-8");
    java.util.HashSet<String> tx=new java.util.HashSet<>();for(String line:trace.split("\n")){JSONObject row=new JSONObject(line);JSONObject d=row.optJSONObject("data");if(row.optString("event").equals("tx")&&d!=null&&d.optString("op").equals("write")){String command=d.getString("command");check(tx.add(command),"Repeated ECM command: "+command);}}
    if(mode.equals("ecm_dtc"))check(tx.contains("03A9811200000000")&&tx.contains("0120000000000000"),"DTC report/session stop absent");
    main(()->check(((TextView)a.hscanDialog.findViewById(android.R.id.content).findViewWithTag("hscan-result")).getText().toString().equals(result.optString("display")),"Displayed result differs from shared core"));
    screenshot(mode.equals("ecm_dtc")?"android-hscan-engine-codes":"android-hscan-ecm-information");
   }else {check(attempts()==before,"Layout-only test opened Bluetooth");screenshot("android-hscan-ready");}
   main(()->a.hscanDialog.dismiss());out.putString("stream","PASS: "+mode+" native ECM probe, "+(live?"fresh VIN, complete shared result, no replay and confirmed cleanup":"layout only, no vehicle request")+"\n");
  }catch(Throwable t){code=0;out.putString("stream","FAIL: "+t+"\n");try{screenshot("android-hscan-failure");}catch(Exception ignored){}}
  finally{if(activity!=null){final Activity a=activity;main(a::finish);}finish(code,out);}
 }
}
