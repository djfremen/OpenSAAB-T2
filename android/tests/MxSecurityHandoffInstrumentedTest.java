// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Instrumentation;import android.os.Bundle;
import java.io.*;import java.net.*;import java.nio.charset.StandardCharsets;import java.nio.file.Files;
import java.util.*;import java.util.concurrent.*;import java.util.concurrent.atomic.AtomicInteger;
import org.json.JSONObject;

/** Real bridge/parked worker, synthetic UART/card only. No Bluetooth API or vehicle. */
public final class MxSecurityHandoffInstrumentedTest extends Instrumentation {
 private static final String ADDRESS="00:04:3E:00:00:01",VIN="YS3FD49YX41000001";
 private void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
 private static final class Endpoint implements VlinkerSppTransport.Endpoint {
  final boolean gated,badIdentity,nativeCleanup;final BlockingQueue<Integer> bytes=new LinkedBlockingQueue<>();volatile Queue<?> transportPackets;
  final List<String> commands=Collections.synchronizedList(new ArrayList<>());
  final AtomicInteger opens=new AtomicInteger(),closes=new AtomicInteger();
  final CountDownLatch probe=new CountDownLatch(1),allow=new CountDownLatch(1),closedLatch=new CountDownLatch(1);
  volatile boolean closed;volatile Thread responder;
  Endpoint(boolean gated,boolean badIdentity,boolean nativeCleanup){this.gated=gated;this.badIdentity=badIdentity;this.nativeCleanup=nativeCleanup;}
  public void connect(){opens.incrementAndGet();}
  void reply(String text){synchronized(bytes){if(!closed)for(byte b:text.getBytes(StandardCharsets.US_ASCII))bytes.offer(b&255);}}
  public InputStream input(){return new InputStream(){public int read()throws IOException{try{return bytes.take();}catch(InterruptedException e){Thread.currentThread().interrupt();throw new IOException(e);}}public int read(byte[] b,int off,int len)throws IOException{int n=read();if(n<0)return -1;b[off]=(byte)n;int count=1;synchronized(bytes){while(count<len){Integer next=bytes.poll();if(next==null)break;if(next<0){bytes.offer(-1);break;}b[off+count++]=(byte)(int)next;}}return count;}};}
  public OutputStream output(){return new OutputStream(){final ByteArrayOutputStream command=new ByteArrayOutputStream();public void write(int b)throws IOException{
   if(closed)throw new IOException("Fixture closed");if(b!=13){command.write(b);return;}
   String text=command.toString("US-ASCII");command.reset();commands.add(text);
   if(nativeCleanup&&text.equals("STPC")){
    // Explicit queue-order fault injection: current delivery locking normally orders
    // completion first. Exercise the bridge's defensive RX-before-completion boundary.
    try{@SuppressWarnings("unchecked") Queue<Object> queued=(Queue<Object>)transportPackets;queued.add(new JSONObject().put("type","rx").put("bytes",new org.json.JSONArray(Arrays.asList(79,75,13,62))));}
    catch(org.json.JSONException error){throw new IOException(error);}return;
   }
   if(!text.equals("ATI"))throw new IOException("Parked UART emitted a non-ATI command");
   if(gated){reply("ATI\rELM327");probe.countDown();responder=new Thread(()->{try{if(!allow.await(4,TimeUnit.SECONDS))return;reply(" v1.4b\r>");}catch(InterruptedException e){Thread.currentThread().interrupt();}},"fixture-ATI-tail");responder.start();}
   else{reply(badIdentity?"garbage\r>":"ELM327 v1.4b\r>");probe.countDown();}
  }};}
  public void close(){if(closes.incrementAndGet()==1){closed=true;allow.countDown();bytes.offer(-1);closedLatch.countDown();}}
 }
 private final class Fixture implements AutoCloseable {
  final Endpoint endpoint;final VlinkerSppTransport link;final MxNativeBridge bridge;final File run,card;
  MxSecurityTransport parked;
  Fixture(boolean gate,boolean bad)throws Exception{this(gate,bad,false);}
  Fixture(boolean gate,boolean bad,boolean nativeCleanup)throws Exception{
   endpoint=new Endpoint(gate,bad,nativeCleanup);link=new VlinkerSppTransport(endpoint,"OBDLink MX+ fixture");link.open();check(link.receive(1000).optString("type").equals("ready"),"Missing initial ready");
   java.lang.reflect.Field packets=VlinkerSppTransport.class.getDeclaredField("packets");packets.setAccessible(true);endpoint.transportPackets=(Queue<?>)packets.get(link);
   bridge=new MxNativeBridge(getTargetContext(),link,SessionModes.Mode.FULL);
   run=new File(getTargetContext().getFilesDir(),"mx-handoff-fixture-"+UUID.randomUUID());check(run.mkdirs(),"Cannot make fixture run");card=new File(run,"card.bin");Files.write(card.toPath(),new byte[]{1,2,3});
  }
  String finish(boolean arm,String request)throws Exception{
   if(arm)bridge.armSecurityHandoff();
   try(Socket peer=new Socket("127.0.0.1",35676)){
    peer.setSoTimeout(3000);BufferedReader in=new BufferedReader(new InputStreamReader(peer.getInputStream(),StandardCharsets.US_ASCII));OutputStream out=peer.getOutputStream();
    out.write(("HELLO "+bridge.token()+"\n").getBytes(StandardCharsets.US_ASCII));out.flush();check("READY obdlink-native-full-native".equals(in.readLine()),"Bridge authority/authentication changed");
    if(endpoint.nativeCleanup){out.write("WRITE STPC\n".getBytes(StandardCharsets.US_ASCII));out.flush();check("WRITTEN".equals(in.readLine()),"Native STPC was not written");out.write("READ\n".getBytes(StandardCharsets.US_ASCII));out.flush();JSONObject rx=new JSONObject(in.readLine());check(rx.optString("type").equals("rx")&&rx.getJSONArray("bytes").length()==4,"Native cleanup prompt was not received before completion");check(!link.readyForHandoff(),"Queued native completion was ignored by ownership proof");}
    out.write((request+"\n").getBytes(StandardCharsets.US_ASCII));out.flush();return in.readLine();
   }
  }
  MxSecurityTransport.Binding binding()throws Exception{
   MxSecurityTransport.Binding context=new MxSecurityTransport.Binding(run,card,ADDRESS,false,VIN);
   check(context.beforeHash==null,"Pre-stop context claimed an import hash");
   Files.write(card.toPath(),new byte[]{2,3,4});return context.stoppedCard();
  }
  MxSecurityTransport park(MxSecurityTransport.Binding binding)throws Exception{check("PARKED".equals(finish(true,"FINISH CLEAN")),"Clean armed finish did not park");parked=bridge.takeSecurityTransport(getTargetContext(),binding,true,true,true);return parked;}
  String imported(MxSecurityTransport.Binding binding)throws Exception{Files.write(card.toPath(),new byte[]{3,4,5});String after=SsaCardImport.hash(card);parked.imported(run,card,binding.beforeHash,after);return after;}
  public void close(){if(parked!=null)parked.close();bridge.close();link.shutdown();Thread reply=endpoint.responder;if(reply!=null)try{reply.join(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();}remove(run);}
 }
 private static void remove(File file){File[] children=file.listFiles();if(children!=null)for(File child:children)remove(child);file.delete();}
 private void ordinaryAndRejectedProof()throws Exception{
  try(Fixture f=new Fixture(false,false,true)){check("PARKED".equals(f.finish(true,"FINISH CLEAN")),"Trailing native STPC completion prevented clean park");check(f.link.readyForHandoff()&&f.endpoint.commands.equals(Collections.singletonList("STPC")),"FINISH CLEAN left unread completion or sent extra commands");}
  for(String request:new String[]{"CLOSE","FINISH CLEAN"})try(Fixture f=new Fixture(false,false)){check("CLOSED".equals(f.finish(false,request)),"Ordinary finish retained UART");f.bridge.close();check(f.link.closeConfirmed()&&f.endpoint.closes.get()==1&&f.endpoint.commands.isEmpty(),"Ordinary finish failed release or sent a command");}
  for(int invalid=0;invalid<3;invalid++)try(Fixture f=new Fixture(false,false)){
   check("PARKED".equals(f.finish(true,"FINISH CLEAN")),"Missing armed park");MxSecurityTransport.Binding binding=f.binding();
   try{f.bridge.takeSecurityTransport(getTargetContext(),binding,invalid!=0,invalid!=1,invalid!=2);throw new AssertionError("Unproven native shutdown retained UART");}catch(IOException expected){}
   f.bridge.close();check(f.link.closeConfirmed()&&f.endpoint.closes.get()==1&&f.endpoint.commands.isEmpty(),"Rejected native proof did not close without probes");
  }
 }
 private void importedClaimQuiesces()throws Exception{
  try(Fixture f=new Fixture(true,false)){
   MxSecurityTransport.Binding binding=f.binding();f.park(binding);check(f.endpoint.probe.await(3,TimeUnit.SECONDS),"Immediate ATI did not run");String after=f.imported(binding);
   CountDownLatch entered=new CountDownLatch(1),done=new CountDownLatch(1);final MxNativeBridge.PreparedTransport[] prepared={null};final Throwable[] failed={null};
   Thread claim=new Thread(()->{entered.countDown();try{prepared[0]=f.parked.claim(f.run,f.card,ADDRESS,false,VIN,after);}catch(Throwable error){failed[0]=error;}finally{done.countDown();}},"fixture-security-claim");claim.start();check(entered.await(1,TimeUnit.SECONDS),"Claim not entered");
   check(!done.await(150,TimeUnit.MILLISECONDS),"Partial ATI prompt was transferred to firmware");f.endpoint.allow.countDown();check(done.await(6,TimeUnit.SECONDS),"Claim did not quiesce the protocol worker");if(failed[0]!=null)throw new AssertionError("Clean import claim failed",failed[0]);
   check(prepared[0]!=null&&f.endpoint.opens.get()==1&&f.endpoint.closes.get()==0,"Claim reopened or closed retained socket");VlinkerSppTransport adopted=prepared[0].claim(ADDRESS,false);check(adopted==f.link&&adopted.readyForHandoff(),"Claim left unread RX/write completion or changed socket");
   try{prepared[0].claim(ADDRESS,false);throw new AssertionError("Prepared socket was claimed twice");}catch(IllegalStateException expected){}
   prepared[0].close();f.bridge.close();check(f.endpoint.closes.get()==0,"Old bridge/wrapper closed transferred UART");
   check(f.endpoint.commands.equals(Collections.singletonList("ATI")),"Parked transport sent vehicle/reset/replay commands: "+f.endpoint.commands);adopted.shutdown();check(adopted.closeConfirmed(),"Adopted socket did not release");
  }
 }
 private void cancelledInvalidIdentityAndImport()throws Exception{
  try(Fixture f=new Fixture(false,true)){f.park(f.binding());check(f.endpoint.closedLatch.await(4,TimeUnit.SECONDS),"Invalid ATI identity retained socket");f.parked.close();check(!f.parked.available()&&f.endpoint.commands.equals(Collections.singletonList("ATI")),"Invalid ATI advanced/replayed");}
  try(Fixture f=new Fixture(false,false)){MxSecurityTransport.Binding binding=f.binding();f.park(binding);check(f.endpoint.probe.await(3,TimeUnit.SECONDS),"Missing ATI before import mismatch");try{f.parked.imported(f.run,f.card,"0".repeat(64),"1".repeat(64));throw new AssertionError("Wrong import hash accepted");}catch(IOException expected){}f.parked.close();check(f.link.closeConfirmed(),"Wrong import hash retained UART");}
  try(Fixture f=new Fixture(true,false)){f.park(f.binding());check(f.endpoint.probe.await(3,TimeUnit.SECONDS),"Missing cancellation ATI");f.parked.close();check(!f.parked.available()&&f.link.closeConfirmed()&&f.endpoint.closes.get()==1,"Cancellation left parked UART/worker active");}
 }
 private void sanitizedHostReceipt()throws Exception{
  File run=new File(getTargetContext().getFilesDir(),"mx-host-receipt-"+UUID.randomUUID());check(run.mkdirs(),"Missing receipt dir");
  try{JSONObject host=new JSONObject().put("adapter","mxplus").put("protocol_cleanup","failed").put("bluetooth_release","retained_for_security").put("security_transport_retained",true).put("workflow_qualified",true).put("device_address","PRIVATE_SECRET").put("error","PRIVATE_SECRET").put("transport_evidence",new JSONObject().put("clock","android_elapsed_realtime").put("release_to_connect_ms",JSONObject.NULL).put("first_probe_write_elapsed_ms",123).put("private","PRIVATE_SECRET"));Files.write(new File(run,"host-release.json").toPath(),host.toString().getBytes(StandardCharsets.UTF_8));JSONObject evidence=DiagnosticEvidence.collect(run,"offline");String json=evidence.toString();check(evidence.getString("adapter").equals("obdlink")&&!json.contains("PRIVATE_SECRET"),"Host receipt adapter/redaction failed");JSONObject safe=evidence.getJSONObject("host_transport");check(!safe.getBoolean("workflow_qualified")&&safe.getString("protocol_cleanup").equals("failed")&&safe.getString("bluetooth_release").equals("retained_for_security")&&safe.getBoolean("security_transport_retained"),"Host receipt upgraded protocol/ECU outcome or lost parked disposition");check(safe.getJSONObject("timing").isNull("release_to_connect_ms")&&safe.getJSONObject("timing").getLong("first_probe_write_elapsed_ms")==123,"Sanitized telemetry lost unknown/known timing");}finally{remove(run);}
 }
 public void onCreate(Bundle arguments){super.onCreate(arguments);start();}
 public void onStart(){Bundle result=new Bundle();try{check(SimulatorVehicleConnection.available(getTargetContext()),"Disposable debug emulator required");ordinaryAndRejectedProof();importedClaimQuiesces();cancelledInvalidIdentityAndImport();sanitizedHostReceipt();result.putString("stream","PASS: production authenticated FINISH CLEAN/CLOSE, clean-only park, stopped-card/import hashes, real adapter-only ATI worker, partial-prompt quiescence, same socket/one-shot transfer, invalid proof/identity/import and cancellation release, sanitized host receipt; synthetic UART/card only, no Bluetooth API or vehicle\n");finish(-1,result);}catch(Throwable error){result.putString("stream","FAIL: "+android.util.Log.getStackTraceString(error));finish(0,result);}}
}
