// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;
import java.util.*;import java.util.concurrent.*;import org.json.*;

/** Exercises the production SPP stream binding with synthetic endpoints, never a radio/helper. */
public final class DirectBluetoothInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle args){super.onCreate(args);start();}
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private void main(Runnable task){runOnMainSync(task);}
 private static String frames(byte[] payload){StringBuilder out=new StringBuilder();int p=0,seq=1;
  while(p<payload.length){byte[] f=new byte[8];int start,count;
   if(p==0&&payload.length<=7){f[0]=(byte)payload.length;start=1;count=payload.length;}
   else if(p==0){f[0]=0x10;f[1]=(byte)payload.length;start=2;count=6;}
   else{f[0]=(byte)(0x20|seq++);start=1;count=Math.min(7,payload.length-p);}
   System.arraycopy(payload,p,f,start,count);p+=count;if(out.length()>0)out.append('\r');out.append("7E8");for(byte b:f)out.append(String.format(Locale.ROOT," %02X",b&255));
  }return out.toString();
 }
 private static final class Endpoint implements VlinkerSppTransport.Endpoint {
  final String mode;final List<String> commands=Collections.synchronizedList(new ArrayList<>());
  final BlockingQueue<Integer> bytes=new LinkedBlockingQueue<>();final CountDownLatch entered=new CountDownLatch(1),requested=new CountDownLatch(1),closedLatch=new CountDownLatch(1);
  volatile boolean closed;final ByteArrayOutputStream wire=new ByteArrayOutputStream(),command=new ByteArrayOutputStream();
  Endpoint(String mode){this.mode=mode;}
  public void connect()throws IOException{entered.countDown();if(mode.equals("blocked-connect"))waitUntilClosed();}
  private void waitUntilClosed()throws IOException{try{closedLatch.await();}catch(InterruptedException e){Thread.currentThread().interrupt();}throw new IOException("Fixture socket closed");}
  public InputStream input(){return new InputStream(){
   public int read()throws IOException{try{return bytes.take();}catch(InterruptedException e){throw new IOException(e);}}
   public int read(byte[] b,int off,int len)throws IOException{int first=read();if(first<0)return -1;b[off]=(byte)first;int n=1;while(n<Math.min(len,7)){Integer next=bytes.poll();if(next==null)break;if(next<0){bytes.offer(-1);break;}b[off+n++]=(byte)(int)next;}return n;}
  };}
  public OutputStream output(){return new OutputStream(){public void write(int b)throws IOException{
   if(closed)throw new IOException("Fixture closed");wire.write(b);
   if(mode.equals("blocked-write"))waitUntilClosed();
   if(b!=13){if(b==10||b<32||b>126)throw new IOException("SPP received non-ASCII/CR framing");command.write(b);return;}
   String cmd=command.toString("US-ASCII");command.reset();commands.add(cmd);
   if(cmd.equals("03A9811200000000")){requested.countDown();if(mode.equals("timeout")||mode.equals("cancel"))return;if(mode.equals("disconnect")){bytes.offer(-1);return;}}
   String reply=cmd.equals("ATZ")?"ELM327":cmd.equals("VTI")?(mode.equals("wrong-adapter")?"Unknown adapter":mode.startsWith("evo-")?"Carista EVO v2.2.84":"vLinker MC+ fixture"):cmd.equals("VTVERS")?(mode.startsWith("evo-")?"MIC3413 V2.0.84":"MIC3313 V2.0.92"):cmd.equals("VTPRON")?"6 [AT]":cmd.equals("VTPBRD")?"500000":cmd.equals("ATRV")?"12.0V":cmd.equals("021A900000000000")?frames(("Z\u0090YS3FD49YX41000001").getBytes(StandardCharsets.ISO_8859_1)):cmd.equals("0210020000000000")?"7E8 01 50 00 00 00 00 00 00":cmd.equals("0120000000000000")?"7E8 01 60 00 00 00 00 00 00":cmd.equals("03A9811200000000")?"5E8 81 01 07 00 6F 00 00 00"+((mode.equals("truncated")||mode.equals("evo-truncated"))?"":"\r5E8 81 00 00 00 FF 00 00 00"):"OK";
   if(cmd.startsWith("021A")&&!cmd.equals("021A900000000000")){
    int pid=Integer.parseInt(cmd.substring(4,6),16);byte[] data=pid==0x95?new byte[]{0,11}:pid==0x7c?new byte[]{0,1,2,3}:"FA5B_C_FMEP_46_FIEF_82d\0".getBytes(StandardCharsets.US_ASCII);byte[] payload=new byte[data.length+2];payload[0]=0x5a;payload[1]=(byte)pid;System.arraycopy(data,0,payload,2,data.length);reply=frames(payload);
   }
   for(byte value:(reply+"\r>").getBytes(StandardCharsets.US_ASCII))bytes.offer(value&255);
  }};}
  public void close()throws IOException{closed=true;closedLatch.countDown();bytes.offer(-1);if(mode.equals("unconfirmed-close"))throw new IOException("Synthetic release failure");}
 }
 private JSONObject run(Context context,String mode,boolean info)throws Exception{
  Endpoint endpoint=new Endpoint(mode);final JSONObject[] terminal={null};
  VlinkerVehicleConnection client=new VlinkerVehicleConnection(context,value->{if(!VlinkerVehicleConnection.active())terminal[0]=value;},()->new VlinkerSppTransport(endpoint),"android-classic-spp",true,true);
  main(()->client.startProbe(info?"ecm_info":"ecm_dtc"));
  if(mode.equals("cancel")){check(endpoint.requested.await(10,TimeUnit.SECONDS),"Missing diagnostic request");VlinkerVehicleConnection overlap=new VlinkerVehicleConnection(context,v->{},()->new VlinkerSppTransport(new Endpoint("complete")),"android-classic-spp",true,true);main(overlap::start);check(!overlap.busy,"Overlapping native connection started");SystemClock.sleep(150);main(client::stop);}
  if(mode.equals("blocked-connect")){check(endpoint.entered.await(10,TimeUnit.SECONDS),"Connect not entered");main(client::stop);}
  long limit=SystemClock.elapsedRealtime()+20000;while(client.busy&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(25);waitForIdleSync();
  check(!client.busy&&!VlinkerVehicleConnection.active(),"Native owner retained: "+mode);check(endpoint.closed,"Socket not closed: "+mode);check(terminal[0]!=null,"Missing native terminal result: "+mode);
  JSONObject report=terminal[0].getJSONObject("report");boolean success=mode.equals("complete")||mode.equals("evo-complete");
  check(report.getString("transport").equals("synthetic-fixture"),"Fixture mislabeled as hardware");check(report.getString("status").equals(success?"complete":"failed"),"Wrong terminal outcome: "+mode+" "+report);
  check(!report.getBoolean("automatic_replay"),"Automatic replay enabled");
  check(report.getBoolean("bluetooth_closed")!=mode.equals("unconfirmed-close"),"Incorrect release receipt: "+mode);
  check(Collections.frequency(endpoint.commands,"021A900000000000")<2,"VIN request replayed");check(Collections.frequency(endpoint.commands,"03A9811200000000")<2,"DTC request replayed");
  if(!mode.equals("blocked-connect")&&!mode.equals("blocked-write")&&!mode.equals("wrong-adapter")){check(Collections.frequency(endpoint.commands,"021A900000000000")==1,"No fresh VIN");}
  if(mode.equals("cancel")||mode.equals("timeout")||(mode.equals("truncated")||mode.equals("evo-truncated"))){check(Collections.frequency(endpoint.commands,"0120000000000000")==1,"Shared diagnostic stop absent/replayed: "+mode+" "+endpoint.commands);check(Collections.frequency(endpoint.commands,"VTPC")==1,"Shared protocol close absent/replayed: "+mode+" "+endpoint.commands);}
  if(mode.equals("wrong-adapter"))check(!endpoint.commands.contains("ATSP6")&&!endpoint.commands.contains("021A900000000000"),"Unverified adapter reached vehicle commands");
  if(success){check(report.getInt("host_vehicle_request_attempts")==1&&report.getInt("host_vehicle_requests_written")==1,"VIN count mismatch");JSONObject result=report.getJSONObject("result");if(mode.startsWith("evo-")){check(result.getString("adapter_model").equals("carista_evo"),"EVO probe lost model");check(result.getString("profile_id").equals(info?"carista_evo_mic3413_hscan_ecm_info_v1":"carista_evo_mic3413_hscan_ecm_dtc_v1"),"EVO probe lost operation profile");}check(result.getString("vin").equals("YS3FD49YX41000001"),"Chunked VIN decode failed");if(info)check(result.getJSONObject("ecm_identifiers").length()==7,"Identifier suite incomplete");else check(result.getJSONObject("dtc_report").getJSONArray("records").length()==1,"Raw DTC decode failed");String id=report.getString("saved_report_id");File text=new File(context.getFilesDir(),"dtc-reports/"+id+".txt"),json=new File(context.getFilesDir(),"dtc-reports/"+id+".json");check(new String(Files.readAllBytes(text.toPath()),StandardCharsets.UTF_8).contains("Synthetic fixture replay"),"Saved report omitted fixture scope");text.delete();json.delete();}
  else check(!report.has("result")&&!report.has("saved_report_id"),"Partial result saved as complete");
  return report;
 }
 private void startup(Context context,String mode)throws Exception{
  // Exercise the normal coordinator with the native stream, then restore prior saved history.
  File history=new File(context.getFilesDir(),"last-vehicle.json");byte[] prior=history.exists()?Files.readAllBytes(history.toPath()):null;
  Endpoint endpoint=new Endpoint(mode);final JSONObject[] terminal={null};
  VlinkerVehicleConnection client=new VlinkerVehicleConnection(context,v->{if(!VlinkerVehicleConnection.active())terminal[0]=v;},()->new VlinkerSppTransport(endpoint),"android-classic-spp",true,true);
  try{main(client::start);long limit=SystemClock.elapsedRealtime()+25000;while(client.busy&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(25);waitForIdleSync();
   check(!client.busy&&!VlinkerVehicleConnection.active()&&endpoint.closed,"Normal startup did not release SPP");check(terminal[0]!=null&&terminal[0].optString("state").equals("identified"),"Normal startup failed: "+terminal[0]);check(terminal[0].getJSONObject("vehicle").getString("vin").equals("YS3FD49YX41000001"),"Startup did not use fresh fixture VIN");check(!terminal[0].getBoolean("vehicle_connected")&&!terminal[0].getBoolean("firmware_running")&&!terminal[0].getBoolean("firmware_transport_available"),"Identification claimed a firmware connection");check(Collections.frequency(endpoint.commands,"021A900000000000")==1&&Collections.frequency(endpoint.commands,"VTPC")==1,"Normal startup replay or missing cleanup");
  }finally{if(client.busy)main(client::stop);if(prior==null)history.delete();else Files.write(history.toPath(),prior);}
 }
 public void onStart(){Bundle result=new Bundle();int code=-1;
  try{Context context=getTargetContext();check(SimulatorVehicleConnection.available(context),"Debug emulator required; never use phone fixtures");
   for(String mode:new String[]{"complete","timeout","cancel","truncated","disconnect","wrong-adapter","blocked-connect","blocked-write","unconfirmed-close","complete"})run(context,mode,false);
   run(context,"complete",true);run(context,"evo-complete",true);run(context,"evo-complete",false);run(context,"evo-truncated",false);
   startup(context,"complete");startup(context,"evo-vin");
   Endpoint blocked=new Endpoint("blocked-connect");VlinkerSppTransport transport=new VlinkerSppTransport(blocked);long started=SystemClock.elapsedRealtime();boolean timedOut=false;try{transport.open();}catch(IOException expected){timedOut=true;}finally{transport.shutdown();}check(timedOut&&blocked.closed&&SystemClock.elapsedRealtime()-started<10000,"Connect deadline failed");
   result.putString("stream","PASS: direct production SPP binding, ASCII/CR and fragmented RX, VIN/7 identifiers/raw codes, timeout, Stop, disconnect, blocked connect/write, unconfirmed close, adapter rejection, single owner and fresh reconnect; 17 synthetic cases including MC+ and EVO VIN, identifiers and DTC profiles, no Bluetooth radio or helper\n");
  }catch(Throwable error){code=0;result.putString("stream","FAIL: "+error+"\n");}finish(code,result);
 }
}
