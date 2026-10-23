// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.util.*;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** One pre-stream retry for the exact Android failure captured with HCI Page Timeout.
 * The enclosing transport owns the overall deadline, cancellation and admission.
 * No stream or protocol command is exposed until a socket connects successfully.
 */
final class SppConnectRecovery {
    interface Endpoint {
        void connect() throws IOException;
        InputStream input() throws IOException;
        OutputStream output() throws IOException;
        void close() throws IOException;
    }
    interface Factory { Endpoint create() throws IOException; }
    interface Guard { boolean retryAllowed(); }
    interface Clock { long now(); }
    static final String CAPTURED_ANDROID_FAILURE="read failed, socket might closed or timeout, read ret: -1";
    static final class Attempt {
        final int ordinal;
        final long started, finished;
        final String outcome, error;
        final Owned socket;
        Attempt(int ordinal,long started,long finished,String outcome,String error,Owned socket){
            this.ordinal=ordinal;this.started=started;this.finished=finished;
            this.outcome=outcome;this.error=error;this.socket=socket;
        }
        boolean closeConfirmed(){return socket.closed.getCount()==0 && socket.closeFailure==null;}
    }
    private static final class Owned {
        final Endpoint endpoint;
        final java.util.concurrent.atomic.AtomicBoolean closing=new java.util.concurrent.atomic.AtomicBoolean();
        final CountDownLatch closed=new CountDownLatch(1);
        volatile IOException closeFailure;
        Owned(Endpoint endpoint){this.endpoint=Objects.requireNonNull(endpoint);}
        void close() throws IOException {
            if(closing.compareAndSet(false,true)){
                try{endpoint.close();}
                catch(IOException e){closeFailure=e;}
                catch(RuntimeException e){closeFailure=new IOException("Bluetooth socket close failed",e);}
                finally{closed.countDown();}
            }else try{
                if(!closed.await(1500,TimeUnit.MILLISECONDS))throw new IOException("Bluetooth socket close is still pending");
            }catch(InterruptedException e){Thread.currentThread().interrupt();throw new IOException("Bluetooth close interrupted",e);}
            if(closeFailure!=null)throw closeFailure;
        }
    }
    private final Object lock=new Object();
    private final Factory factory;
    private final Guard guard;
    private final Clock clock;
    private final List<Attempt> attempts=new ArrayList<>();
    private Owned current;
    private boolean cancelled, invoked, connected;
    private IOException releaseFailure;

    SppConnectRecovery(Endpoint first,Factory factory,Guard guard,Clock clock){
        current=new Owned(first);this.factory=factory;this.guard=guard;this.clock=clock;
    }
    private void available() throws IOException {
        if(cancelled||Thread.currentThread().isInterrupted())throw new IOException("Bluetooth connection cancelled");
    }
    void connect() throws IOException {
        synchronized(lock){available();if(invoked)throw new IOException("Bluetooth connect already attempted");invoked=true;}
        for(int ordinal=1;ordinal<=2;ordinal++){
            Owned socket;
            synchronized(lock){available();socket=current;}
            long start=clock.now();
            try{
                socket.endpoint.connect();
                synchronized(lock){
                    attempts.add(new Attempt(ordinal,start,clock.now(),"connected","",socket));
                    available();connected=true;
                }
                return;
            }catch(IOException failure){
                // A cancelled success is already recorded; do not invent a second open.
                synchronized(lock){if(attempts.size()<ordinal)attempts.add(new Attempt(ordinal,start,clock.now(),"failed",failure.toString(),socket));}
                if(ordinal!=1||!CAPTURED_ANDROID_FAILURE.equals(failure.getMessage()))throw failure;
                synchronized(lock){available();}
                if(!guard.retryAllowed())throw failure;
                try{socket.close();}
                catch(IOException closeError){synchronized(lock){releaseFailure=closeError;}closeError.addSuppressed(failure);throw closeError;}
                synchronized(lock){available();}
                if(!guard.retryAllowed())throw failure;
                // Factory calls can race cancellation. A late socket must be closed,
                // never connected or made available after the owner cancels.
                Owned replacement=new Owned(factory.create());
                synchronized(lock){
                    if(!cancelled&&!Thread.currentThread().isInterrupted()){current=replacement;continue;}
                }
                try{replacement.close();}
                catch(IOException closeError){synchronized(lock){releaseFailure=closeError;}throw closeError;}
                throw new IOException("Bluetooth connection cancelled");
            }
        }
        throw new IOException("Bluetooth connection attempts exhausted");
    }
    private Endpoint connectedEndpoint() throws IOException {
        synchronized(lock){available();if(!connected)throw new IOException("Bluetooth streams unavailable before connection");return current.endpoint;}
    }
    InputStream input() throws IOException {return connectedEndpoint().input();}
    OutputStream output() throws IOException {return connectedEndpoint().output();}
    void close() throws IOException {
        Owned socket;
        synchronized(lock){cancelled=true;socket=current;}
        socket.close();
        synchronized(lock){if(releaseFailure!=null)throw releaseFailure;}
    }
    List<Attempt> attempts(){synchronized(lock){return new ArrayList<>(attempts);}}
}
