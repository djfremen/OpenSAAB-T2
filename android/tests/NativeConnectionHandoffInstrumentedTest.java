// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Instrumentation;
import android.os.Bundle;
import java.io.*;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

/** Synthetic byte endpoint only. Never opens a Bluetooth API, radio or vehicle. */
public final class NativeConnectionHandoffInstrumentedTest extends Instrumentation {
    private static void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    private static final class Endpoint implements VlinkerSppTransport.Endpoint {
        final BlockingQueue<Integer> bytes=new LinkedBlockingQueue<>();
        final List<String> commands=Collections.synchronizedList(new ArrayList<>());
        int opens,closes;boolean closed;
        public void connect(){check(++opens==1,"Second socket open reproduced the captured failure");}
        public InputStream input(){return new InputStream(){
            public int read()throws IOException{try{return bytes.take();}catch(InterruptedException e){throw new IOException(e);}}
            public int read(byte[] b,int off,int len)throws IOException{int first=read();if(first<0)return -1;b[off]=(byte)first;int n=1;while(n<Math.min(7,len)){Integer next=bytes.poll();if(next==null)break;if(next<0){bytes.offer(-1);break;}b[off+n++]=(byte)(int)next;}return n;}
        };}
        public OutputStream output(){return new OutputStream(){final ByteArrayOutputStream pending=new ByteArrayOutputStream();
            public void write(int b)throws IOException{
                if(closed)throw new IOException("Fixture closed");if(b!=13){pending.write(b);return;}
                String cmd=pending.toString("US-ASCII");pending.reset();commands.add(cmd);
                String reply=cmd.equals("ATZ")?"ELM327 v1.4b":cmd.equals("STDI")?"OBDLink MX+ r3.1.3":cmd.equals("STI")?"STN2255 v5.12.4":cmd.equals("ATRV")?"12.4V":cmd.equals("STPR")?"6":cmd.equals("STPBRR")?"500000":cmd.equals("021A900000000000")?"7E8 10 13 5A 90 59 53 33 46\r7E8 21 44 34 39 59 58 34 31\r7E8 22 30 30 30 30 30 31 00":"OK";
                for(byte v:(reply+"\r\r>").getBytes(StandardCharsets.US_ASCII))bytes.offer(v&255);
            }
        };}
        public synchronized void close(){if(!closed){closed=true;closes++;bytes.offer(-1);}}
    }
    private void scenario(boolean adopt)throws Exception{
        Endpoint endpoint=new Endpoint();VlinkerSppTransport link=new VlinkerSppTransport(endpoint,"OBDLink MX+ fixture");
        Field address=VlinkerSppTransport.class.getDeclaredField("deviceAddress");address.setAccessible(true);address.set(link,"00:04:3E:00:00:01");
        Field mode=VlinkerSppTransport.class.getDeclaredField("connectionMode");mode.setAccessible(true);mode.set(link,"authenticated-spp");
        CountDownLatch done=new CountDownLatch(1);AtomicReference<Throwable> failed=new AtomicReference<>();
        VlinkerVehicleConnection[] owner=new VlinkerVehicleConnection[1];VlinkerSppTransport[] adopted=new VlinkerSppTransport[1];
        owner[0]=new VlinkerVehicleConnection(getTargetContext(),snapshot->{if(!snapshot.optBoolean("startup_terminal"))return;
            try{
                String id=snapshot.getString("attempt_id");check(snapshot.getString("state").equals("identified"),"Shared startup rejected handoff: "+snapshot);
                JSONObject report=snapshot.getJSONObject("startup_report");check(!report.getBoolean("bluetooth_closed"),"Retained socket falsely reported closed");
                check(owner[0].nativeTransportReady(id)&&VlinkerVehicleConnection.active(),"Retained socket lost exclusive discovery ownership");
                check(endpoint.opens==1&&endpoint.closes==0,"Socket closed/reopened between VIN and callback");
                if(adopt){
                    MxNativeBridge.PreparedTransport prepared=owner[0].takeNativeTransport(id);
                    Field leaseField=prepared.getClass().getDeclaredField("lease");leaseField.setAccessible(true);
                    @SuppressWarnings("unchecked") NativeTransportLease<VlinkerSppTransport> lease=(NativeTransportLease<VlinkerSppTransport>)leaseField.get(prepared);
                    adopted[0]=lease.claim(id,"00:04:3E:00:00:01",false);check(adopted[0]==link,"Native adoption replaced transport");
                    check(!VlinkerVehicleConnection.active(),"Discovery ownership not released after adoption");prepared.close();check(endpoint.closes==0,"Wrapper closed adopted socket");
                }
            }catch(Throwable error){failed.set(error);}finally{done.countDown();}
        },()->link,"android-classic-spp",true);
        Field retain=VlinkerVehicleConnection.class.getDeclaredField("retainForNative");retain.setAccessible(true);retain.setBoolean(owner[0],true);
        owner[0].start();check(done.await(25,TimeUnit.SECONDS),"Terminal callback timed out");
        runOnMainSync(()->{}); // include callback's finally/discard in the observation
        if(adopted[0]!=null){check(endpoint.closes==0,"Discovery closed the adopted socket after callback");adopted[0].shutdown();}
        owner[0].stop();if(failed.get()!=null)throw new AssertionError("Handoff fixture",failed.get());
        check(endpoint.opens==1&&endpoint.closes==1&&!VlinkerVehicleConnection.active(),"Final cleanup leaked or reopened the transport");
        check(Collections.frequency(endpoint.commands,"021A900000000000")==1,"VIN request replayed");check(Collections.frequency(endpoint.commands,"STPC")==1,"Discovery protocol cleanup not confirmed once");
    }
    @Override public void onCreate(Bundle arguments){super.onCreate(arguments);start();}
    @Override public void onStart(){Bundle result=new Bundle();try{scenario(true);scenario(false);result.putString("stream","PASS: synthetic production startup/retained-socket adoption and unclaimed cleanup; one open, one VIN, one STPC, no Bluetooth API/vehicle\n");finish(-1,result);}catch(Throwable error){result.putString("stream","FAIL: "+android.util.Log.getStackTraceString(error));finish(0,result);}}
}
