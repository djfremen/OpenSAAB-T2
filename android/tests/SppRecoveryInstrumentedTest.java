// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.os.*;
import org.json.*;
import java.io.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;

/** Production Java transport, synthetic endpoints only; no Bluetooth permission. */
public final class SppRecoveryInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle args){super.onCreate(args);start();}
 private static void check(boolean b,String why){if(!b)throw new AssertionError(why);}
 private static final class Socket implements SppConnectRecovery.Endpoint {
  final AtomicInteger opens=new AtomicInteger(),closes=new AtomicInteger(),streamGets=new AtomicInteger();
  final CountDownLatch entered=new CountDownLatch(1),closed=new CountDownLatch(1);
  final boolean fail,block,badClose;Socket(boolean fail,boolean block,boolean badClose){this.fail=fail;this.block=block;this.badClose=badClose;}
  public void connect()throws IOException{
   opens.incrementAndGet();entered.countDown();
   if(block)try{closed.await();throw new IOException("fixture closed");}catch(InterruptedException e){throw new IOException(e);}
   if(fail)throw new IOException(SppConnectRecovery.CAPTURED_ANDROID_FAILURE);
  }
  public InputStream input(){streamGets.incrementAndGet();return new InputStream(){public int read()throws IOException{try{closed.await();return -1;}catch(InterruptedException e){throw new IOException(e);}}};}
  public OutputStream output(){streamGets.incrementAndGet();return new ByteArrayOutputStream();}
  public void close()throws IOException{closes.incrementAndGet();closed.countDown();if(badClose)throw new IOException("unconfirmed fixture socket close");}
 }
 private static SppConnectRecovery recovery(Socket first,Socket second){return new SppConnectRecovery(first,()->{check(first.closes.get()==1,"Socket replaced before failed socket close");return second;},()->true,SystemClock::elapsedRealtime);}
 private static void join(Thread t)throws Exception{t.join(4000);check(!t.isAlive(),"Cancelled connect did not exit");}
 public void onStart(){Bundle result=new Bundle();int status=-1;
  try{
   check(Build.FINGERPRINT.contains("generic")||Build.MODEL.contains("sdk_gphone"),"Emulator fixture only");
   Socket first=new Socket(true,false,false),second=new Socket(false,false,false);VlinkerSppTransport transport=new VlinkerSppTransport(recovery(first,second));
   transport.open();check(transport.receive(1000).getString("type").equals("ready"),"Recovered transport not ready");JSONObject e=transport.connectionEvidence();
   check(e.getInt("connect_retry_count")==1&&e.getJSONArray("socket_attempts").length()==2,"Attempt history absent from production evidence");
   check(first.streamGets.get()==0&&transport.connectionEvidence().getInt("transport_write_attempts")==0,"Failed socket exposed streams/writes");
   transport.send(new JSONObject().put("op","write").put("command","ATI"));check(transport.receive(1000).getString("type").equals("write_completed"),"Recovered socket did not carry explicit later probe");
   transport.shutdown();check(transport.closeConfirmed()&&first.closes.get()==1&&second.closes.get()==1,"Recovered sockets not released");

   first=new Socket(true,false,true);second=new Socket(false,false,false);VlinkerSppTransport sticky=new VlinkerSppTransport(recovery(first,second));
   try{sticky.open();throw new AssertionError("Unconfirmed release accepted");}catch(IOException expected){}finally{sticky.shutdown();}
   check(second.opens.get()==0&&!sticky.closeConfirmed(),"Unconfirmed first close lost admission barrier");

   first=new Socket(true,false,false);second=new Socket(false,true,false);VlinkerSppTransport cancel=new VlinkerSppTransport(recovery(first,second));AtomicReference<Throwable> error=new AtomicReference<>();
   Thread thread=new Thread(()->{try{cancel.open();}catch(Throwable failure){error.set(failure);}});thread.start();check(second.entered.await(3,TimeUnit.SECONDS),"Retry not entered");cancel.cancelPendingIO();join(thread);cancel.shutdown();
   check(error.get() instanceof IOException&&cancel.closeConfirmed()&&second.closes.get()==1,"Stop during retry leaked socket or kept connecting");

   first=new Socket(true,false,false);second=new Socket(false,true,false);VlinkerSppTransport timed=new VlinkerSppTransport(recovery(first,second));long start=SystemClock.elapsedRealtime();boolean expired=false;
   try{timed.open();}catch(IOException expected){expired=expected.getMessage().equals("Bluetooth connect timed out");}finally{timed.shutdown();}
   long elapsed=SystemClock.elapsedRealtime()-start;
   check(expired&&elapsed>=20000&&elapsed<25000&&timed.closeConfirmed(),"Retry reset/escaped overall 20second watchdog");
   check(timed.connectionEvidence().getInt("transport_write_attempts")==0&&timed.connectionEvidence().getInt("connect_retry_count")==1,"Timed retry invented writes or lost attempts");
   result.putString("stream","PASS: production SPP pre-stream retry history, first socket release, sticky close guard, Stop during second open, unchanged overall20s deadline, no automatic protocol/vehicle writes\n");
  }catch(Throwable error){status=0;result.putString("stream","FAIL: "+error+"\n");}finish(status,result);
 }
}
