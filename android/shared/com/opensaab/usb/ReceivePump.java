// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.util.Arrays;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.TimeUnit;

/** Drain USB independently of firmware scheduling. Overflow fails, never drops bytes. */
final class ReceivePump {
    interface Source {
        int read(byte[] buffer) throws IOException;
        void stop() throws IOException;
    }
    private final ArrayBlockingQueue<byte[]> packets;
    private final Thread worker;
    private volatile boolean stopping;
    private volatile IOException failure;
    private volatile IOException cleanupFailure;
    private volatile long received;
    private volatile int highWater;
    ReceivePump(Source source,int packetSize,int capacity) {
        if(packetSize<1 || packetSize>1024 || capacity<1 || capacity>4096)throw new IllegalArgumentException();
        packets=new ArrayBlockingQueue<>(capacity);
        worker=new Thread(()->{
            byte[] buffer=new byte[packetSize];
            try {
                while(!stopping) {
                    int n=source.read(buffer);
                    if(n<0 || n>buffer.length)throw new IOException("Invalid USB receive length");
                    if(n==0)continue;
                    if(!packets.offer(Arrays.copyOf(buffer,n)))throw new IOException("USB receive queue overflow; stream incomplete, session stopped");
                    received+=n;highWater=Math.max(highWater,packets.size());
                }
            } catch(IOException e){failure=e;}
            catch(RuntimeException e){failure=new IOException("USB receive worker failed",e);}
            finally {try{source.stop();}catch(IOException e){cleanupFailure=e;if(failure==null)failure=e;}}
        },"nano-usb-receive");
        worker.setDaemon(true);
        worker.start();
    }
    void checkHealthy() throws IOException {if(failure!=null)throw failure;}
    int read(byte[] buffer,int timeoutMs) throws IOException {
        checkHealthy();
        try {
            byte[] next=packets.poll(timeoutMs,TimeUnit.MILLISECONDS);
            checkHealthy();
            if(next==null)return 0;
            if(next.length>buffer.length)throw new IOException("USB receive destination too small");
            System.arraycopy(next,0,buffer,0,next.length);return next.length;
        }catch(InterruptedException e){Thread.currentThread().interrupt();throw new IOException("USB receive interrupted",e);}
    }
    void stop() throws IOException {
        stopping=true;
        try{worker.join(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();throw new IOException("USB receiver stop interrupted",e);}
        if(worker.isAlive())throw new IOException("USB receiver did not stop");
        if(cleanupFailure!=null)throw cleanupFailure;
    }
    String stats(){return "received_bytes="+received+" queued_packets="+packets.size()+" queue_high_water="+highWater+" failure="+(failure==null?"none":failure.getMessage());}
}
