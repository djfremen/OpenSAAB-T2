// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.os.*;import android.content.*;
import java.io.*;import java.net.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;
import java.util.*;import java.util.concurrent.*;import org.json.*;

/** Native transport fault fixtures. Emulator loopback only; no Bluetooth or vehicle access. */
public final class SimulatorRecoveryInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle args){super.onCreate(args);start();}
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private void main(Runnable task){runOnMainSync(task);}
 private static String frame(int id,byte[] bytes){StringBuilder s=new StringBuilder(String.format(Locale.ROOT,"%03X",id));for(byte b:bytes)s.append(String.format(Locale.ROOT," %02X",b&255));return s.toString();}
 private static String vin(){
  byte[] payload=("Z\u0090YS3FD49YX41000001").getBytes(StandardCharsets.ISO_8859_1);StringBuilder s=new StringBuilder();int p=0,seq=1;
  byte[] first=new byte[8];first[0]=0x10;first[1]=(byte)payload.length;System.arraycopy(payload,0,first,2,6);p=6;s.append(frame(0x7e8,first));
  while(p<payload.length){byte[] next=new byte[8];next[0]=(byte)(0x20|seq++);int count=Math.min(7,payload.length-p);System.arraycopy(payload,p,next,1,count);p+=count;s.append('\r').append(frame(0x7e8,next));}return s.toString();
 }
 private static final class Fixture implements AutoCloseable {
  final ServerSocket server;final List<String> commands=Collections.synchronizedList(new ArrayList<>());final CountDownLatch requested=new CountDownLatch(1);final String mode;volatile Throwable failure;
  Fixture(String mode)throws Exception{this.mode=mode;server=new ServerSocket(0,1,InetAddress.getByName("127.0.0.1"));new Thread(this::serve,"local-vlinker-fixture").start();}
  void packet(PrintWriter out,JSONObject value){out.println(value.toString());out.flush();}
  void serve(){try(Socket link=server.accept();BufferedReader input=new BufferedReader(new InputStreamReader(link.getInputStream(),StandardCharsets.UTF_8));PrintWriter out=new PrintWriter(new OutputStreamWriter(link.getOutputStream(),StandardCharsets.UTF_8))){
   for(String line;(line=input.readLine())!=null;){JSONObject event=new JSONObject(line);String op=event.optString("op");
    if(op.equals("open")){packet(out,new JSONObject().put("type","ready").put("service","18F0").put("write","2AF1").put("notify","2AF0").put("write_type","withResponse"));continue;}
    if(op.equals("close")){packet(out,new JSONObject().put("type","closed").put("bluetooth_closed",true));break;}
    String cmd=event.getString("command");commands.add(cmd);packet(out,new JSONObject().put("type","write_completed").put("command",cmd));
    if(cmd.equals("03A9811200000000")){requested.countDown();if(mode.equals("timeout")||mode.equals("cancel"))continue;if(mode.equals("disconnect")){packet(out,new JSONObject().put("type","closed").put("bluetooth_closed",true).put("error","Synthetic disconnect"));break;}}
    String reply=cmd.equals("ATZ")?"ELM327":cmd.equals("VTI")?"vLinker MC+ fixture":cmd.equals("VTVERS")?"MIC3313 V2.0.92":cmd.equals("VTPRON")?"6 [AT]":cmd.equals("VTPBRD")?"500000":cmd.equals("ATRV")?"12.0V":cmd.equals("021A900000000000")?vin():cmd.equals("0210020000000000")?"7E8 01 50 00 00 00 00 00 00":cmd.equals("0120000000000000")?"7E8 01 60 00 00 00 00 00 00":cmd.equals("03A9811200000000")?"5E8 81 01 07 00 6F 00 00 00"+(mode.equals("truncated")?"":"\r5E8 81 00 00 00 FF 00 00 00"):"OK";
    byte[] bytes=(reply+"\r>").getBytes(StandardCharsets.US_ASCII);JSONArray data=new JSONArray();for(byte b:bytes)data.put(b&255);packet(out,new JSONObject().put("type","rx").put("bytes",data));
   }
  }catch(Throwable error){failure=error;}}
  public void close()throws Exception{server.close();}
 }
 public void onStart(){Bundle result=new Bundle();int code=-1;
  try{
   Context context=getTargetContext();check(SimulatorVehicleConnection.available(context),"Debug emulator core required");
   for(String mode:new String[]{"timeout","cancel","truncated","disconnect","complete"}){
    final JSONObject[] terminal={null};try(Fixture server=new Fixture(mode)){
     SimulatorVehicleConnection client=new SimulatorVehicleConnection(context,value->{if(!clientBusy())terminal[0]=value;},server.server.getLocalPort());
     main(()->client.startProbe("ecm_dtc"));check(server.requested.await(10,TimeUnit.SECONDS),"Fixture did not receive DTC request: "+mode);
     if(mode.equals("timeout")||mode.equals("cancel")){
      SimulatorVehicleConnection overlap=new SimulatorVehicleConnection(context,value->{},server.server.getLocalPort());main(()->overlap.startProbe("ecm_dtc"));check(!overlap.busy,"Overlap started another worker");
     }
     if(mode.equals("cancel"))main(client::stop);
     long deadline=SystemClock.elapsedRealtime()+16000;while(client.busy&&SystemClock.elapsedRealtime()<deadline)SystemClock.sleep(50);waitForIdleSync();
     check(!client.busy&&!SimulatorVehicleConnection.active(),"Owner retained after native release: "+mode);check(terminal[0]!=null,"Missing terminal result");
     JSONObject report=terminal[0].getJSONObject("report");check(report.getBoolean("bluetooth_closed"),"No release confirmation");
     check(report.getString("status").equals(mode.equals("complete")?"complete":"failed"),"Fault published success: "+mode);
     check(Collections.frequency(server.commands,"03A9811200000000")==1,"DTC request replayed");
     if(!mode.equals("disconnect")){check(Collections.frequency(server.commands,"0120000000000000")==1,"Timeout/cancel stop effect was lost or replayed: "+mode);check(Collections.frequency(server.commands,"VTPC")==1,"Protocol close missing");}
     if(mode.equals("complete")){String id=report.getString("saved_report_id");String saved=new String(Files.readAllBytes(new File(context.getFilesDir(),"dtc-reports/"+id+".txt").toPath()),StandardCharsets.UTF_8);check(saved.contains("Saved observation")&&saved.contains("P0107 · failure 00 · status 6F"),"Durable report missing raw scope/status");new File(context.getFilesDir(),"dtc-reports/"+id+".txt").delete();new File(context.getFilesDir(),"dtc-reports/"+id+".json").delete();}
     else check(!report.has("result")&&!report.has("saved_report_id"),"Failure retained completed result");
     check(server.failure==null,"Fixture transport failed: "+server.failure);
    }
   }
   result.putString("stream","PASS: native timeout, cancel, truncated report, disconnect, overlap rejection and fresh reconnect; saved complete report; emulator-local fixtures only\n");
  }catch(Throwable error){code=0;result.putString("stream","FAIL: "+error+"\n");}
  finish(code,result);
 }
 private boolean clientBusy(){return SimulatorVehicleConnection.active();}
}
