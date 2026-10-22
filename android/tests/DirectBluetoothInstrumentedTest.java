// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;
import java.util.*;import java.util.concurrent.*;import org.json.*;
import java.util.concurrent.atomic.AtomicInteger;

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
 private static class Endpoint implements VlinkerSppTransport.Endpoint {
  final String mode;final List<String> commands=Collections.synchronizedList(new ArrayList<>());
  final BlockingQueue<Integer> bytes=new LinkedBlockingQueue<>();final CountDownLatch entered=new CountDownLatch(1),requested=new CountDownLatch(1),closedLatch=new CountDownLatch(1);
  final AtomicInteger socketCloses=new AtomicInteger(),inputCloses=new AtomicInteger(),outputCloses=new AtomicInteger();
  final AtomicInteger opens=new AtomicInteger();
  String lastCommand="03A9811200000000";int readyRepeats,atAttempts;
  volatile boolean closed;final ByteArrayOutputStream wire=new ByteArrayOutputStream(),command=new ByteArrayOutputStream();
  Endpoint(String mode){this.mode=mode;}
  public void connect()throws IOException{if(opens.incrementAndGet()!=1)throw new IOException("Fixture socket was opened twice");entered.countDown();if(mode.equals("blocked-connect"))waitUntilClosed();}
  private void waitUntilClosed()throws IOException{try{closedLatch.await();}catch(InterruptedException e){Thread.currentThread().interrupt();}throw new IOException("Fixture socket closed");}
  public InputStream input(){return new InputStream(){
   public int read()throws IOException{try{return bytes.take();}catch(InterruptedException e){throw new IOException(e);}}
   public int read(byte[] b,int off,int len)throws IOException{int first=read();if(first<0)return -1;b[off]=(byte)first;int n=1;while(n<Math.min(len,7)){Integer next=bytes.poll();if(next==null)break;if(next<0){bytes.offer(-1);break;}b[off+n++]=(byte)(int)next;}return n;}
   public void close(){inputCloses.incrementAndGet();bytes.offer(-1);}
  };}
  public OutputStream output(){return new OutputStream(){public void write(int b)throws IOException{
   if(closed)throw new IOException("Fixture closed");wire.write(b);
   if(mode.equals("blocked-write"))waitUntilClosed();
   if(b!=13){if(b==10||b<32||b>126)throw new IOException("SPP received non-ASCII/CR framing");command.write(b);return;}
   String raw=command.toString("US-ASCII");command.reset();commands.add(raw);
   // A bare CR repeats the previous adapter command, which is deliberately a vehicle request initially.
   String cmd=raw.isEmpty()?lastCommand:raw;if(raw.isEmpty())readyRepeats++;else if(raw.equals("AT"))atAttempts++;
   boolean monitorConsumesAt=mode.startsWith("mxp-stm-")&&cmd.equals("AT")&&(atAttempts==1||mode.equals("mxp-stm-twice"));
   if(!raw.isEmpty()&&!monitorConsumesAt)lastCommand=raw;
   if(monitorConsumesAt){String residual=mode.equals("mxp-stm-one")?"STOPPED\r>":mode.equals("mxp-stm-noise")?"STOPPED\r>garbage":mode.equals("mxp-stm-incomplete")?"STOPPED\r>?":"STOPPED\r>?\r>";for(byte value:residual.getBytes(StandardCharsets.US_ASCII))bytes.offer(value&255);return;}
   if(cmd.equals("03A9811200000000")){requested.countDown();if(mode.equals("timeout")||mode.equals("cancel"))return;if(mode.equals("disconnect")){bytes.offer(-1);return;}}
   String reply=cmd.equals("ATZ")?"ELM327":cmd.equals("ATI")?"ELM327 v1.4b":cmd.equals("STDI")?"OBDLink MX+ r3.1.3":cmd.equals("STI")?"STN2255 v5.12.4":cmd.equals("STPR")?"6":cmd.equals("STPBRR")?"500000":cmd.equals("VTI")?(mode.equals("wrong-adapter")?"Unknown adapter":mode.startsWith("evo-")?"Carista EVO v2.2.84":"vLinker MC+ fixture"):cmd.equals("VTVERS")?(mode.startsWith("evo-")?"MIC3413 V2.0.84":"MIC3313 V2.0.92"):cmd.equals("VTPRON")?"6 [AT]":cmd.equals("VTPBRD")?"500000":cmd.equals("ATRV")?"12.0V":cmd.equals("021A900000000000")?frames(("Z\u0090YS3FD49YX41000001").getBytes(StandardCharsets.ISO_8859_1)):cmd.equals("0210020000000000")?"7E8 01 50 00 00 00 00 00 00":cmd.equals("0120000000000000")?"7E8 01 60 00 00 00 00 00 00":cmd.equals("03A9811200000000")?"5E8 81 01 07 00 6F 00 00 00"+((mode.equals("truncated")||mode.equals("evo-truncated"))?"":"\r5E8 81 00 00 00 FF 00 00 00"):"OK";
   if(cmd.equals("AT")){
    if(mode.equals("mxp-cancel-repeat")&&readyRepeats==1){requested.countDown();return;}
    if(mode.equals("mxp-fail-at")&&readyRepeats==0)reply="?";
    if(mode.equals("mxp-fail-repeat")&&readyRepeats==2)reply="?";
    if(mode.equals("mxp-busy-banner")&&readyRepeats==0)reply="BUS BUSY";
    reply="AT\r"+reply; // Echo is enabled before ATE0, including repeated AT.
   }
   if(cmd.equals("ATI")&&mode.equals("mxp-fail-ati"))reply="Unknown adapter";
   if(cmd.startsWith("021A")&&!cmd.equals("021A900000000000")){
    int pid=Integer.parseInt(cmd.substring(4,6),16);byte[] data=pid==0x95?new byte[]{0,11}:pid==0x7c?new byte[]{0,1,2,3}:"FA5B_C_FMEP_46_FIEF_82d\0".getBytes(StandardCharsets.US_ASCII);byte[] payload=new byte[data.length+2];payload[0]=0x5a;payload[1]=(byte)pid;System.arraycopy(data,0,payload,2,data.length);reply=frames(payload);
   }
   for(byte value:(reply+(mode.equals("mxp-repeat-no-prompt")&&readyRepeats==2&&cmd.equals("AT")?"\r":"\r>")).getBytes(StandardCharsets.US_ASCII))bytes.offer(value&255);
  }public void close(){outputCloses.incrementAndGet();}};}
  public void close()throws IOException{socketCloses.incrementAndGet();closed=true;closedLatch.countDown();bytes.offer(-1);if(mode.equals("unconfirmed-close"))throw new IOException("Synthetic release failure");}
 }
 /** Independent stream/socket closures and a reader that can outlive close, unlike a socket-only mock. */
 private static final class CleanupEndpoint implements VlinkerSppTransport.Endpoint {
  final String mode;
  final AtomicInteger sockets=new AtomicInteger(),inputs=new AtomicInteger(),outputs=new AtomicInteger(),inputRequests=new AtomicInteger();
  final CountDownLatch reading=new CountDownLatch(1),readerExit=new CountDownLatch(1),closing=new CountDownLatch(1),allowClose=new CountDownLatch(1),outputRequested=new CountDownLatch(1),allowOutput=new CountDownLatch(1);
  final CountDownLatch writerEntered=new CountDownLatch(1),writerExit=new CountDownLatch(1);
  volatile Thread reader;int chunks;
  CleanupEndpoint(String mode){this.mode=mode;}
  private static void await(CountDownLatch latch)throws IOException{try{if(!latch.await(5,TimeUnit.SECONDS))throw new IOException("Fixture latch timed out");}catch(InterruptedException e){Thread.currentThread().interrupt();throw new IOException(e);}}
  public void connect(){}
  public InputStream input(){inputRequests.incrementAndGet();return new InputStream(){
   public int read()throws IOException{reader=Thread.currentThread();reading.countDown();if(mode.equals("overflow")&&chunks++<80)return 65;await(readerExit);return -1;}
   public int read(byte[] b,int off,int len)throws IOException{int n=read();if(n<0)return -1;b[off]=(byte)n;return 1;}
   public void close()throws IOException{inputs.incrementAndGet();if(!mode.equals("reader-timeout"))readerExit.countDown();if(mode.equals("input-failure"))throw new IOException("fixture input close failure");}
  };}
  public OutputStream output()throws IOException{outputRequested.countDown();if(mode.equals("late-output"))await(allowOutput);return new OutputStream(){
   public void write(int value)throws IOException{if(mode.equals("held-writer")){writerEntered.countDown();await(writerExit);throw new IOException("Fixture writer unwound after socket close");}}
   public void close()throws IOException{outputs.incrementAndGet();if(mode.equals("output-failure"))throw new IOException("fixture output close failure");}
  };}
  public void close()throws IOException{sockets.incrementAndGet();closing.countDown();if(mode.equals("concurrent"))await(allowClose);if(!mode.equals("reader-timeout"))readerExit.countDown();if(mode.equals("socket-failure"))throw new IOException("fixture socket close failure");}
 }
 private void joined(Thread thread,String why)throws Exception{thread.join(4000);check(!thread.isAlive(),why);}
 private void admissionBlocked(SppTransportAdmission admission,String why)throws Exception{
  try{admission.reserve(()->true);throw new AssertionError(why);}
  catch(IOException blocked){check(blocked.getMessage().contains("force stop OpenSAAB in Android settings"),"Unconfirmed release omitted actionable process-restart guidance");}
 }
 private void releaseTiming()throws Exception{
  final java.util.concurrent.atomic.AtomicLong now=new java.util.concurrent.atomic.AtomicLong(100);SppReleaseTiming.Clock clock=now::get;SppReleaseTiming.Ledger ledger=new SppReleaseTiming.Ledger();
  CleanupEndpoint endpoint=new CleanupEndpoint("timing");VlinkerSppTransport first=new VlinkerSppTransport(endpoint,"fixture",ledger.attempt("A","boot-1",clock));
  first.open();first.receive(1000);check(endpoint.reading.await(2,TimeUnit.SECONDS),"Timing fixture reader absent");
  JSONObject start=first.connectionEvidence();check(start.getLong("connect_started_elapsed_ms")==100&&start.isNull("prior_confirmed_release_elapsed_ms")&&start.isNull("release_to_connect_ms"),"Unknown release became an inferred interval");
  now.set(110);first.send(new JSONObject().put("op","write").put("command","AT"));first.receive(1000);
  now.set(120);first.send(new JSONObject().put("op","write").put("command","021A900000000000"));first.receive(1000);
  now.set(130);first.shutdown();JSONObject released=first.connectionEvidence();
  check(released.getLong("first_probe_write_elapsed_ms")==110&&released.getLong("first_vehicle_write_elapsed_ms")==120&&released.getLong("release_started_elapsed_ms")==130&&released.getLong("confirmed_release_elapsed_ms")==130,"Connect/probe/write/release times were conflated");
  now.set(150);CleanupEndpoint nextEndpoint=new CleanupEndpoint("timing-next");VlinkerSppTransport next=new VlinkerSppTransport(nextEndpoint,"fixture",ledger.attempt("A","boot-1",clock));
  next.open();next.receive(1000);JSONObject evidence=next.connectionEvidence();check(evidence.getLong("prior_confirmed_release_elapsed_ms")==130&&evidence.getLong("release_to_connect_ms")==20&&evidence.getLong("transport_write_attempts")==0,"Release-to-connect telemetry changed transport effects");
  now.set(160);next.shutdown();
 }
 private void cleanupOwnership()throws Exception{
  CleanupEndpoint clean=new CleanupEndpoint("clean");VlinkerSppTransport transport=new VlinkerSppTransport(clean);transport.open();
  check(clean.reading.await(2,TimeUnit.SECONDS),"Clean reader never entered");transport.receive(1000);
  transport.cancelPendingIO();check(!clean.closing.await(50,TimeUnit.MILLISECONDS)&&transport.readyForHandoff(),"Idle cancellation reset the active handoff socket");
  transport.send(new JSONObject().put("op","write").put("command","AT"));check(!transport.readyForHandoff(),"Unread write completion qualified for handoff");check(transport.receive(1000).optString("type").equals("write_completed"),"Idle cancellation broke existing stream");check(transport.readyForHandoff(),"Drained completion did not restore idle ownership");
  transport.send(new JSONObject().put("op","close"));check(transport.receive(1000).getBoolean("bluetooth_closed"),"Close packet omitted confirmed release");transport.shutdown();
  check(clean.sockets.get()==1&&clean.inputs.get()==1&&clean.outputs.get()==1&&!clean.reader.isAlive()&&transport.closeConfirmed(),"Socket/streams/reader were not released exactly once");

  CleanupEndpoint concurrent=new CleanupEndpoint("concurrent");VlinkerSppTransport parallel=new VlinkerSppTransport(concurrent);parallel.open();check(concurrent.reading.await(2,TimeUnit.SECONDS),"Concurrent reader missing");
  Thread first=new Thread(parallel::shutdown),second=new Thread(parallel::shutdown);first.start();check(concurrent.closing.await(2,TimeUnit.SECONDS),"First closer absent");second.start();
  check(!parallel.closeConfirmed(),"In-flight concurrent close reported release");concurrent.allowClose.countDown();joined(first,"First close deadlocked");joined(second,"Concurrent close deadlocked");
  check(parallel.closeConfirmed()&&concurrent.sockets.get()==1&&concurrent.inputs.get()==1&&concurrent.outputs.get()==1&&!concurrent.reader.isAlive(),"Concurrent close lost/duplicated ownership");

  CleanupEndpoint held=new CleanupEndpoint("held-writer");VlinkerSppTransport unwinding=new VlinkerSppTransport(held);unwinding.open();check(held.reading.await(2,TimeUnit.SECONDS),"Held writer reader missing");unwinding.receive(1000);
  SppTransportAdmission admission=new SppTransportAdmission();admission.reserve(unwinding);admission.constructed(unwinding);admissionBlocked(admission,"Active transport allowed another SPP owner");
  final Throwable[] writeOutcome={null};Thread writer=new Thread(()->{try{unwinding.send(new JSONObject().put("op","write").put("command","AT"));}catch(Throwable error){writeOutcome[0]=error;}});writer.start();check(held.writerEntered.await(2,TimeUnit.SECONDS),"Writer did not enter fixture");
  try{unwinding.shutdown();check(!held.reader.isAlive()&&writer.isAlive()&&!unwinding.closeConfirmed(),"Unwinding writer was incorrectly reported released after reader/socket close");check(unwinding.connectionEvidence().isNull("confirmed_release_elapsed_ms"),"Live writer published a release timestamp");admissionBlocked(admission,"Unwinding writer admitted another socket after higher-level owner exit");}
  finally{held.writerExit.countDown();joined(writer,"Writer did not unwind after fixture release");unwinding.shutdown();}
  check(writeOutcome[0] instanceof IOException&&unwinding.closeConfirmed()&&held.sockets.get()==1&&held.inputs.get()==1&&held.outputs.get()==1,"Writer release lost original stream/socket ownership");
  check(!unwinding.connectionEvidence().isNull("confirmed_release_elapsed_ms"),"Confirmed writer release omitted its timestamp");
  SppTransportAdmission.Owner replacement=()->true;admission.reserve(replacement);admission.constructed(replacement);admission.requireOwner(replacement);

  for(String mode:new String[]{"socket-failure","input-failure","output-failure"}){
   CleanupEndpoint broken=new CleanupEndpoint(mode);VlinkerSppTransport rejected=new VlinkerSppTransport(broken);rejected.open();check(broken.reading.await(2,TimeUnit.SECONDS),"Failure reader missing");rejected.shutdown();rejected.shutdown();
   check(!rejected.closeConfirmed()&&broken.sockets.get()==1&&broken.inputs.get()==1&&broken.outputs.get()==1&&!broken.reader.isAlive(),"Release failure hidden or later resources skipped: "+mode);
   check(!rejected.connectionEvidence().getString("release_failure").isEmpty(),"Release failure evidence missing: "+mode);
   check(rejected.connectionEvidence().isNull("confirmed_release_elapsed_ms"),"Failed release published a confirmed time: "+mode);
   SppTransportAdmission sticky=new SppTransportAdmission();sticky.reserve(rejected);sticky.constructed(rejected);admissionBlocked(sticky,"Sticky "+mode+" failure allowed another SPP socket");
  }

  CleanupEndpoint delayed=new CleanupEndpoint("reader-timeout");VlinkerSppTransport pending=new VlinkerSppTransport(delayed);pending.open();check(delayed.reading.await(2,TimeUnit.SECONDS),"Delayed reader missing");
  try{long started=SystemClock.elapsedRealtime();pending.send(new JSONObject().put("op","close"));long elapsed=SystemClock.elapsedRealtime()-started;
   check(elapsed>=VlinkerSppTransport.READER_STOP_TIMEOUT_MS-100&&elapsed<3000,"Reader join did not respect its bound");
   check(!pending.receive(1000).getBoolean("bluetooth_closed")&&!pending.closeConfirmed()&&delayed.reader.isAlive(),"Live reader incorrectly reported released");
  }finally{delayed.readerExit.countDown();joined(delayed.reader,"Fixture delayed reader survived");pending.shutdown();}
  check(!pending.closeConfirmed(),"Reader termination deadline failure was forgotten");

  CleanupEndpoint overflow=new CleanupEndpoint("overflow");VlinkerSppTransport full=new VlinkerSppTransport(overflow);full.open();check(overflow.reading.await(2,TimeUnit.SECONDS),"Overflow reader missing");joined(overflow.reader,"Reader queue-overflow close self-joined/deadlocked");full.shutdown();
  check(full.closeConfirmed()&&overflow.sockets.get()==1&&overflow.inputs.get()==1&&overflow.outputs.get()==1,"Reader self-close lost release ownership");
  check(full.connectionEvidence().getString("failure").contains("queue exceeded"),"Receive overflow root failure lost");

  CleanupEndpoint late=new CleanupEndpoint("late-output");VlinkerSppTransport cancelled=new VlinkerSppTransport(late);final Throwable[] outcome={null};
  Thread opener=new Thread(()->{try{cancelled.open();}catch(Throwable error){outcome[0]=error;}});opener.start();check(late.outputRequested.await(2,TimeUnit.SECONDS),"Late stream acquisition missing");
  cancelled.shutdown();check(!cancelled.closeConfirmed(),"Unfinished stream acquisition reported released");late.allowOutput.countDown();joined(opener,"Late acquisition ignored cancellation");cancelled.shutdown();
  check(outcome[0] instanceof IOException&&cancelled.closeConfirmed()&&late.sockets.get()==1&&late.outputs.get()==1&&late.inputRequests.get()==0,"Late stream leaked or cancelled startup acquired input");
 }
 private void wakePrefix(Endpoint endpoint,boolean complete){
  List<String> wanted=endpoint.atAttempts==2?Arrays.asList("AT","AT","","","ATI","ATD"):Arrays.asList("AT","","","ATI","ATD");
  check(!endpoint.commands.contains("ATZ")&&!endpoint.commands.contains("ATWS"),"MX+ wake rebooted adapter");
  int count=complete?wanted.size():Math.min(wanted.size(),endpoint.commands.size());
  check(endpoint.commands.size()>=count&&endpoint.commands.subList(0,count).equals(wanted.subList(0,count)),"Unsafe MX+ wake/repeat prefix: "+endpoint.commands);
  if(complete)check(Collections.frequency(endpoint.commands,"")==2&&endpoint.readyRepeats==2,"Readiness CR repeated a vehicle request or replayed beyond the two confirmed probes");
 }
 private void failedWake(Context context,String mode)throws Exception{
  Endpoint endpoint=new Endpoint(mode);final JSONObject[] terminal={null};
  VlinkerVehicleConnection client=new VlinkerVehicleConnection(context,v->{if(!VlinkerVehicleConnection.active())terminal[0]=v;},()->new VlinkerSppTransport(endpoint,"OBDLink MX+ 63988"),"android-classic-spp",true,true);
  main(()->client.startProbe("ecm_dtc"));
  if(mode.equals("mxp-cancel-repeat")){check(endpoint.requested.await(5,TimeUnit.SECONDS),"First confirmed AT did not reach repeat");main(client::stop);}
  long limit=SystemClock.elapsedRealtime()+10000;while(client.busy&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(25);waitForIdleSync();
  check(!client.busy&&!VlinkerVehicleConnection.active()&&terminal[0]!=null,"Failed wake retained native owner: "+mode);
  JSONObject report=terminal[0].getJSONObject("report");check(report.optString("status").equals("failed")&&!report.has("result")&&!report.has("saved_report_id")&&report.getBoolean("bluetooth_closed"),"Wake failure published success or omitted cleanup: "+mode);
  check(!endpoint.commands.contains("021A900000000000")&&!endpoint.commands.contains("03A9811200000000")&&!endpoint.commands.contains("ATD"),"Failed readiness advanced to setup/vehicle request: "+endpoint.commands);
  wakePrefix(endpoint,false);check(endpoint.socketCloses.get()==1&&endpoint.inputCloses.get()==1&&endpoint.outputCloses.get()==1,"Wake failure leaked or duplicated stream close");
  int expected=mode.equals("mxp-fail-at")||mode.equals("mxp-stm-twice")?2:mode.equals("mxp-busy-banner")||mode.startsWith("mxp-stm-")?1:mode.equals("mxp-fail-ati")?4:mode.equals("mxp-cancel-repeat")?2:3;
  check(endpoint.commands.size()==expected,"Readiness advanced/replayed after its failure: "+mode+" "+endpoint.commands);
  if(mode.equals("mxp-repeat-no-prompt"))check(report.optString("error").contains("timeout"),"Missing prompt failed for the wrong reason: "+report);
  if(mode.equals("mxp-cancel-repeat"))check(endpoint.commands.equals(Arrays.asList("AT","")),"Cancellation after confirmed AT advanced/replayed readiness: "+endpoint.commands);
 }
 private JSONObject run(Context context,String mode,boolean info)throws Exception{
  Endpoint endpoint=new Endpoint(mode);final JSONObject[] terminal={null};
  VlinkerVehicleConnection client=new VlinkerVehicleConnection(context,value->{if(!VlinkerVehicleConnection.active())terminal[0]=value;},()->new VlinkerSppTransport(endpoint,mode.startsWith("mxp-")?"OBDLink MX+ 63988":""),"android-classic-spp",true,true);
  main(()->client.startProbe(info?"ecm_info":"ecm_dtc"));
  if(mode.equals("cancel")){check(endpoint.requested.await(10,TimeUnit.SECONDS),"Missing diagnostic request");VlinkerVehicleConnection overlap=new VlinkerVehicleConnection(context,v->{},()->new VlinkerSppTransport(new Endpoint("complete")),"android-classic-spp",true,true);main(overlap::start);check(!overlap.busy,"Overlapping native connection started");SystemClock.sleep(150);main(client::stop);}
  if(mode.equals("blocked-connect")){check(endpoint.entered.await(10,TimeUnit.SECONDS),"Connect not entered");main(client::stop);}
  long limit=SystemClock.elapsedRealtime()+20000;while(client.busy&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(25);waitForIdleSync();
  check(!client.busy&&!VlinkerVehicleConnection.active(),"Native owner retained: "+mode);check(endpoint.closed,"Socket not closed: "+mode);check(terminal[0]!=null,"Missing native terminal result: "+mode);
  check(endpoint.socketCloses.get()==1&&endpoint.inputCloses.get()==(mode.equals("blocked-connect")?0:1)&&endpoint.outputCloses.get()==(mode.equals("blocked-connect")?0:1),"Socket/stream ownership mismatch: "+mode);
  if(mode.startsWith("mxp-"))wakePrefix(endpoint,true);
  JSONObject report=terminal[0].getJSONObject("report");boolean success=mode.equals("complete")||mode.equals("evo-complete")||mode.equals("mxp-complete")||mode.equals("mxp-stm-one")||mode.equals("mxp-stm-two");
  check(report.getString("transport").equals("synthetic-fixture"),"Fixture mislabeled as hardware");check(report.getString("status").equals(success?"complete":"failed"),"Wrong terminal outcome: "+mode+" "+report);
  check(!report.getBoolean("automatic_replay"),"Automatic replay enabled");
  check(report.getBoolean("bluetooth_closed")!=mode.equals("unconfirmed-close"),"Incorrect release receipt: "+mode);
  check(Collections.frequency(endpoint.commands,"021A900000000000")<2,"VIN request replayed");check(Collections.frequency(endpoint.commands,"03A9811200000000")<2,"DTC request replayed");
  if(!mode.equals("blocked-connect")&&!mode.equals("blocked-write")&&!mode.equals("wrong-adapter")){check(Collections.frequency(endpoint.commands,"021A900000000000")==1,"No fresh VIN");}
  if(mode.equals("cancel")||mode.equals("timeout")||(mode.equals("truncated")||mode.equals("evo-truncated"))){check(Collections.frequency(endpoint.commands,"0120000000000000")==1,"Shared diagnostic stop absent/replayed: "+mode+" "+endpoint.commands);check(Collections.frequency(endpoint.commands,"VTPC")==1,"Shared protocol close absent/replayed: "+mode+" "+endpoint.commands);}
  if(mode.equals("wrong-adapter"))check(!endpoint.commands.contains("ATSP6")&&!endpoint.commands.contains("021A900000000000"),"Unverified adapter reached vehicle commands");
  if(success){check(report.getInt("host_vehicle_request_attempts")==1&&report.getInt("host_vehicle_requests_written")==1,"VIN count mismatch");JSONObject result=report.getJSONObject("result");if(mode.startsWith("evo-")){check(result.getString("adapter_model").equals("carista_evo"),"EVO probe lost model");check(result.getString("profile_id").equals(info?"carista_evo_mic3413_hscan_ecm_info_v1":"carista_evo_mic3413_hscan_ecm_dtc_v1"),"EVO probe lost operation profile");}if(mode.startsWith("mxp-")){check(result.getString("adapter_model").equals("obdlink_mxp"),"MX+ probe lost model");check(result.getString("command_family").equals("STN"),"MX+ probe lost STN command family");check(Collections.frequency(endpoint.commands,"STPC")==1,"MX+ probe missing STPC");}check(result.getString("vin").equals("YS3FD49YX41000001"),"Chunked VIN decode failed");if(info)check(result.getJSONObject("ecm_identifiers").length()==7,"Identifier suite incomplete");else check(result.getJSONObject("dtc_report").getJSONArray("records").length()==1,"Raw DTC decode failed");String id=report.getString("saved_report_id");File text=new File(context.getFilesDir(),"dtc-reports/"+id+".txt"),json=new File(context.getFilesDir(),"dtc-reports/"+id+".json");check(new String(Files.readAllBytes(text.toPath()),StandardCharsets.UTF_8).contains("Synthetic fixture replay"),"Saved report omitted fixture scope");text.delete();json.delete();}
  else check(!report.has("result")&&!report.has("saved_report_id"),"Partial result saved as complete");
  if(mode.equals("mxp-stm-one")||mode.equals("mxp-stm-two")){JSONObject wake=report.getJSONObject("result").getJSONObject("wake_readiness");check(wake.getInt("adapter_only_resync_count")==1&&wake.getInt("initial_AT_attempts")==2&&wake.getInt("residual_prompt_count")== (mode.equals("mxp-stm-one")?1:2),"Residual recovery evidence missing");check(Collections.frequency(endpoint.commands,"AT")==2,"Residual STM was flushed/replayed instead of explicit AT resync");}
  return report;
 }
 private void startup(Context context,String mode)throws Exception{
  // Exercise the normal coordinator with the native stream, then restore prior saved history.
  File history=new File(context.getFilesDir(),"last-vehicle.json");byte[] prior=history.exists()?Files.readAllBytes(history.toPath()):null;
  Endpoint endpoint=new Endpoint(mode);final JSONObject[] terminal={null};
  VlinkerVehicleConnection client=new VlinkerVehicleConnection(context,v->{if(!VlinkerVehicleConnection.active())terminal[0]=v;},()->new VlinkerSppTransport(endpoint,mode.startsWith("mxp-")?"OBDLink MX+ 63988":""),"android-classic-spp",true,true);
  try{main(client::start);long limit=SystemClock.elapsedRealtime()+25000;while(client.busy&&SystemClock.elapsedRealtime()<limit)SystemClock.sleep(25);waitForIdleSync();
   check(!client.busy&&!VlinkerVehicleConnection.active()&&endpoint.closed,"Normal startup did not release SPP");check(terminal[0]!=null&&terminal[0].optString("state").equals("identified"),"Normal startup failed: "+terminal[0]);check(terminal[0].optBoolean("startup_terminal")&&terminal[0].getJSONObject("startup_report").getBoolean("bluetooth_closed"),"No released terminal discovery receipt");check(terminal[0].getJSONObject("vehicle").getString("vin").equals("YS3FD49YX41000001"),"Startup did not use fresh fixture VIN");check(!terminal[0].getBoolean("vehicle_connected")&&!terminal[0].getBoolean("firmware_running")&&!terminal[0].getBoolean("firmware_transport_available"),"Identification claimed a firmware connection");check(Collections.frequency(endpoint.commands,"021A900000000000")==1&&Collections.frequency(endpoint.commands,mode.startsWith("mxp-")?"STPC":"VTPC")==1,"Normal startup replay or missing cleanup");
   if(mode.startsWith("mxp-"))wakePrefix(endpoint,true);
  }finally{if(client.busy)main(client::stop);if(prior==null)history.delete();else Files.write(history.toPath(),prior);}
 }
 private void retainedFreshVin(Context context)throws Exception{
  Endpoint endpoint=new Endpoint("mxp-complete");VlinkerSppTransport transport=new VlinkerSppTransport(endpoint,"OBDLink MX+ fixture");String address="00:04:3E:00:00:01";
  java.lang.reflect.Field device=VlinkerSppTransport.class.getDeclaredField("deviceAddress");device.setAccessible(true);device.set(transport,address);
  java.lang.reflect.Field mode=VlinkerSppTransport.class.getDeclaredField("connectionMode");mode.setAccessible(true);mode.set(transport,"authenticated-spp");
  VlinkerVehicleConnection[] owner={null};MxNativeBridge.PreparedTransport[] prepared={null};Throwable[] failure={null};CountDownLatch[] done={new CountDownLatch(1)};
  VlinkerVehicleConnection.Listener listener=snapshot->{if(!snapshot.optBoolean("startup_terminal"))return;try{check(snapshot.optString("state").equals("identified"),"Retained fresh VIN failed: "+snapshot);prepared[0]=owner[0].takeNativeTransport(snapshot.getString("attempt_id"));}catch(Throwable error){failure[0]=error;}finally{done[0].countDown();}};
  // The outer DebugAVD check supplies the fixture guard; exercise the unchanged
  // production handoff proof format without qualifying these synthetic bytes.
  owner[0]=new VlinkerVehicleConnection(context,listener,()->transport,"android-classic-spp",true,false);
  java.lang.reflect.Field retain=VlinkerVehicleConnection.class.getDeclaredField("retainForNative");retain.setAccessible(true);retain.setBoolean(owner[0],true);
  try{
   main(owner[0]::start);check(done[0].await(25,TimeUnit.SECONDS),"First retained VIN timed out");waitForIdleSync();if(failure[0]!=null)throw new AssertionError(failure[0]);check(prepared[0]!=null&&endpoint.opens.get()==1&&endpoint.socketCloses.get()==0,"Discovery did not retain a quiescent socket");
   MxNativeBridge.PreparedTransport supplied=prepared[0];prepared[0]=null;done[0]=new CountDownLatch(1);
   owner[0]=VlinkerVehicleConnection.debugRetainedFixture(context,listener,supplied,address,false);
   main(owner[0]::start);check(done[0].await(25,TimeUnit.SECONDS),"Retained second VIN timed out");waitForIdleSync();if(failure[0]!=null)throw new AssertionError(failure[0]);
   check(prepared[0]!=null&&endpoint.opens.get()==1&&Collections.frequency(endpoint.commands,"021A900000000000")==2&&Collections.frequency(endpoint.commands,"STPC")==2,"Retained path reopened socket or reused an old VIN/protocol receipt");
  }finally{if(prepared[0]!=null)prepared[0].close();main(owner[0]::stop);transport.shutdown();}
  check(!VlinkerVehicleConnection.active()&&endpoint.socketCloses.get()==1,"Retained fresh VIN cleanup lost ownership");
 }
 public void onStart(){Bundle result=new Bundle();int code=-1;
  try{Context context=getTargetContext();check(SimulatorVehicleConnection.available(context),"Debug emulator required; never use phone fixtures");
   cleanupOwnership();
   releaseTiming();
   for(String mode:new String[]{"complete","timeout","cancel","truncated","disconnect","wrong-adapter","blocked-connect","blocked-write","unconfirmed-close","complete"})run(context,mode,false);
   run(context,"complete",true);run(context,"evo-complete",true);run(context,"evo-complete",false);run(context,"evo-truncated",false);run(context,"mxp-complete",true);run(context,"mxp-complete",false);
   run(context,"mxp-stm-one",false);run(context,"mxp-stm-two",false);
   startup(context,"complete");startup(context,"evo-vin");startup(context,"mxp-vin");
   retainedFreshVin(context);
   for(String mode:new String[]{"mxp-fail-at","mxp-fail-ati","mxp-fail-repeat","mxp-busy-banner","mxp-repeat-no-prompt","mxp-cancel-repeat"})failedWake(context,mode);
   for(String mode:new String[]{"mxp-stm-twice","mxp-stm-noise","mxp-stm-incomplete"})failedWake(context,mode);
   Endpoint refused=new Endpoint("platform-refusal"){
    public void connect()throws IOException{throw new IOException("read failed, socket might closed or timeout, read ret: -1");}
   };
   VlinkerSppTransport refusal=new VlinkerSppTransport(refused);
   try{refusal.open();throw new AssertionError("Refused RFCOMM connect accepted");}
   catch(IOException expected){check(expected.getMessage().contains("No adapter or vehicle requests were sent"),"Connect failure claims an unknown cause");}
   finally{refusal.shutdown();}
   JSONObject failure=refusal.connectionEvidence();
   check(failure.getString("phase").equals("connect_failed")&&failure.getBoolean("failed_before_commands"),"Connect failure layer lost");
   check(failure.getLong("transport_write_attempts")==0&&refused.commands.isEmpty(),"Failed connect wrote/replayed commands");
   check(!failure.isNull("connect_started_elapsed_ms")&&failure.isNull("first_probe_write_elapsed_ms")&&failure.isNull("first_vehicle_write_elapsed_ms"),"Connect refusal timing implied a transmitted probe/vehicle request");
   check(failure.getString("radio_sleep_state").equals("unverified")&&failure.isNull("hci_failure_status"),"Timing guessed radio cause");
   check(failure.getString("platform_error").contains("read ret: -1")&&!failure.getString("failure").isEmpty(),"Original platform error or explanation lost");
   Endpoint blocked=new Endpoint("blocked-connect");VlinkerSppTransport transport=new VlinkerSppTransport(blocked);long started=SystemClock.elapsedRealtime();boolean timedOut=false;try{transport.open();}catch(IOException expected){timedOut=expected.getMessage().equals("Bluetooth connect timed out");}finally{transport.shutdown();}long elapsed=SystemClock.elapsedRealtime()-started;check(timedOut&&blocked.closed&&elapsed>=20000&&elapsed<25000,"Connect deadline failed");
   result.putString("stream","PASS: production SPP binding and rebuilt shared worker; explicit once-only socket/input/output ownership, concurrent close, late stream acquisition, reader/concurrent-closer wait deadline, writer-unwind ownership, reader self-close and sticky release failures; local production admission guard rejects active/unwinding/sticky-failed transports and permits a new owner only after actual writer release; ASCII/CR, fragmented RX, MC+/EVO/MX+ VIN/identifiers/DTC, deadlines, Stop, adapter rejection, fresh reconnect; MX+ AT/two confirmed echo-on repeats/ATI/ATD prefix and six fail-closed readiness cases including missing prompt and operator Stop; no Bluetooth radio or helper\n");
  }catch(Throwable error){code=0;result.putString("stream","FAIL: "+error+"\n");}finish(code,result);
 }
}
