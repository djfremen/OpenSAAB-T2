// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.IOException;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

public class ReceivePumpTest {
    static void check(boolean value){if(!value)throw new AssertionError();}
    static void pause(){try{Thread.sleep(2);}catch(InterruptedException e){Thread.currentThread().interrupt();}}
    public static void main(String[] args)throws Exception {
        CountDownLatch produced=new CountDownLatch(200);AtomicBoolean stopped=new AtomicBoolean();
        ReceivePump pump=new ReceivePump(new ReceivePump.Source(){int sequence;
            public int read(byte[] b){if(sequence==200){pause();return 0;}b[0]=(byte)sequence;b[1]=(byte)(sequence>>8);sequence++;produced.countDown();return 2;}
            public void stop(){stopped.set(true);}
        },64,256);
        // No consumer requests until the producer has drained all source packets.
        check(produced.await(2,TimeUnit.SECONDS));byte[] b=new byte[64];
        for(int i=0;i<200;i++){check(pump.read(b,100)==2);check((b[0]&255)+((b[1]&255)<<8)==i);}
        check(pump.read(b,5)==0);pump.stop();check(stopped.get());
        CountDownLatch overflowClosed=new CountDownLatch(1);
        ReceivePump overflow=new ReceivePump(new ReceivePump.Source(){
            public int read(byte[] b){b[0]=1;return 1;}public void stop(){overflowClosed.countDown();}
        },64,2);
        check(overflowClosed.await(2,TimeUnit.SECONDS));
        try{overflow.read(b,1);throw new AssertionError("Overflow silently delivered partial stream");}catch(IOException expected){check(expected.getMessage().contains("overflow"));}
        overflow.stop();
        CountDownLatch failureClosed=new CountDownLatch(1);
        ReceivePump failed=new ReceivePump(new ReceivePump.Source(){
            public int read(byte[] b)throws IOException{throw new IOException("device disconnected");}
            public void stop(){failureClosed.countDown();}
        },64,2);
        check(failureClosed.await(2,TimeUnit.SECONDS));
        try{failed.checkHealthy();throw new AssertionError();}catch(IOException expected){check(expected.getMessage().contains("disconnected"));}failed.stop();
        ReceivePump cleanup=new ReceivePump(new ReceivePump.Source(){
            public int read(byte[] b){pause();return 0;}public void stop()throws IOException{throw new IOException("cancel failed");}
        },64,2);
        try{cleanup.stop();throw new AssertionError();}catch(IOException expected){check(expected.getMessage().contains("cancel failed"));}
        System.out.println("RECEIVE_PUMP PASS: delayed consumer, ordered bytes, overflow, disconnect, cancellation; no hardware");
    }
}
