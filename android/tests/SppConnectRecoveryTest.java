// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;

public final class SppConnectRecoveryTest {
    private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    private static final class Socket implements SppConnectRecovery.Endpoint {
        final AtomicInteger connects=new AtomicInteger(),closes=new AtomicInteger(),streams=new AtomicInteger();
        IOException connectError,closeError;Runnable beforeConnect=()->{};
        public void connect()throws IOException{check(closes.get()==0,"Closed socket reused");connects.incrementAndGet();beforeConnect.run();if(connectError!=null)throw connectError;}
        public InputStream input(){streams.incrementAndGet();return new ByteArrayInputStream(new byte[0]);}
        public OutputStream output(){streams.incrementAndGet();return new ByteArrayOutputStream();}
        public void close()throws IOException{closes.incrementAndGet();if(closeError!=null)throw closeError;}
    }
    private static Socket pagingFailure(){Socket s=new Socket();s.connectError=new IOException(SppConnectRecovery.CAPTURED_ANDROID_FAILURE);return s;}
    private static void await(CountDownLatch latch){try{check(latch.await(3,TimeUnit.SECONDS),"Fixture coordination timed out");}catch(InterruptedException e){throw new AssertionError(e);}}
    private static void join(Thread t)throws Exception{t.join(3000);check(!t.isAlive(),"Connect did not stop");}
    private static void blockedStreams(SppConnectRecovery r)throws Exception{
        try{r.output();throw new AssertionError("Stream exposed before successful connect");}catch(IOException expected){}
        try{r.input();throw new AssertionError("Stream exposed before successful connect");}catch(IOException expected){}
    }
    public static void main(String[] args)throws Exception{
        AtomicLong now=new AtomicLong(100);Socket first=pagingFailure(),second=new Socket();AtomicInteger created=new AtomicInteger();
        first.beforeConnect=()->now.set(12914);second.beforeConnect=()->now.set(18931);
        SppConnectRecovery recovery=new SppConnectRecovery(first,()->{check(first.closes.get()==1,"Second socket created before first release");created.incrementAndGet();return second;},()->true,now::get);
        blockedStreams(recovery);recovery.connect();
        check(created.get()==1&&first.connects.get()==1&&second.connects.get()==1,"Captured Page Timeout did not get exactly one fresh retry");
        check(first.streams.get()==0&&second.streams.get()==0,"Retry obtained streams or wrote adapter data");
        check(recovery.attempts().size()==2&&recovery.attempts().get(0).outcome.equals("failed")&&recovery.attempts().get(0).error.contains("read ret: -1"),"Recovered result erased initial error");
        check(recovery.attempts().get(0).closeConfirmed()&&!recovery.attempts().get(1).closeConfirmed(),"Live second socket reported released");
        check(recovery.attempts().get(0).finished-recovery.attempts().get(0).started==12814&&recovery.attempts().get(1).finished-recovery.attempts().get(1).started==6017,"Attempt timings collapsed into one connect");
        recovery.output();recovery.input();recovery.close();recovery.close();
        check(first.closes.get()==1&&second.closes.get()==1&&recovery.attempts().get(1).closeConfirmed(),"Repeated cleanup reclosed/leaked a socket");
        try{recovery.connect();throw new AssertionError("Closed recovery opened again");}catch(IOException expected){}

        for(String error:new String[]{"Authentication failed","Permission denied","Service discovery failed","Bluetooth connection cancelled","read failed, different error"}){
            Socket s=new Socket();s.connectError=new IOException(error);AtomicInteger factories=new AtomicInteger();
            SppConnectRecovery r=new SppConnectRecovery(s,()->{factories.incrementAndGet();return new Socket();},()->true,now::get);
            try{r.connect();throw new AssertionError("Failure accepted");}catch(IOException e){check(e.getMessage().equals(error),"Failure classification changed");}
            check(factories.get()==0,"Unknown/auth/cancel failure retried");r.close();
        }
        Socket bad1=pagingFailure(),bad2=pagingFailure();AtomicInteger factories=new AtomicInteger();
        SppConnectRecovery twice=new SppConnectRecovery(bad1,()->{factories.incrementAndGet();return bad2;},()->true,now::get);
        try{twice.connect();throw new AssertionError("Two failures accepted");}catch(IOException expected){}
        check(factories.get()==1&&twice.attempts().size()==2,"More than two opens");twice.close();check(bad1.closes.get()==1&&bad2.closes.get()==1,"Failed opens leaked");

        Socket badClose=pagingFailure();badClose.closeError=new IOException("release unconfirmed");AtomicInteger afterBadClose=new AtomicInteger();
        SppConnectRecovery uncertain=new SppConnectRecovery(badClose,()->{afterBadClose.incrementAndGet();return new Socket();},()->true,now::get);
        try{uncertain.connect();throw new AssertionError("Retried unconfirmed close");}catch(IOException expected){}
        check(afterBadClose.get()==0&&!uncertain.attempts().get(0).closeConfirmed(),"Unconfirmed release permitted replacement");
        try{uncertain.close();throw new AssertionError("Cleanup erased release error");}catch(IOException expected){}

        SppConnectRecovery disallowed=new SppConnectRecovery(pagingFailure(),()->{throw new AssertionError("Bond/adapter guard bypassed");},()->false,now::get);
        try{disallowed.connect();throw new AssertionError("Guard ignored");}catch(IOException expected){}disallowed.close();

        Socket cancelled=pagingFailure();SppConnectRecovery before=new SppConnectRecovery(cancelled,()->{throw new AssertionError("Cancelled transport retried");},()->true,now::get);
        before.close();try{before.connect();throw new AssertionError("Cancelled socket opened");}catch(IOException expected){}check(cancelled.connects.get()==0,"Cancel before open had effects");

        CountDownLatch creating=new CountDownLatch(1),finishCreate=new CountDownLatch(1);Socket original=pagingFailure(),late=new Socket();AtomicReference<Throwable> error=new AtomicReference<>();
        SppConnectRecovery racing=new SppConnectRecovery(original,()->{creating.countDown();await(finishCreate);return late;},()->true,now::get);
        Thread worker=new Thread(()->{try{racing.connect();}catch(Throwable e){error.set(e);}});worker.start();await(creating);racing.close();finishCreate.countDown();join(worker);
        check(error.get() instanceof IOException&&late.connects.get()==0&&late.closes.get()==1,"Late-created socket escaped cancellation");blockedStreams(racing);

        Socket initial=new Socket();AtomicInteger notNeeded=new AtomicInteger();SppConnectRecovery once=new SppConnectRecovery(initial,()->{notNeeded.incrementAndGet();return new Socket();},()->true,now::get);once.connect();check(notNeeded.get()==0&&once.attempts().size()==1,"Success retried");once.close();
        System.out.println("SppConnectRecoveryTest passed: captured failure/recovery, bounded failures, release errors, scope, cancellation and late socket cleanup");
    }
}
