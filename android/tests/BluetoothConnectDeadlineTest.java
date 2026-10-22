// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;

public final class BluetoothConnectDeadlineTest {
    private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    private static final class Timer extends ScheduledThreadPoolExecutor {
        Runnable callback;long delay;
        Timer(){super(1);}
        @Override public ScheduledFuture<?> schedule(Runnable action,long duration,TimeUnit unit){
            callback=action;delay=unit.toMillis(duration);
            return new ScheduledFuture<Object>(){
                public boolean cancel(boolean interrupt){return true;}
                public boolean isCancelled(){return false;}
                public boolean isDone(){return false;}
                public Object get(){return null;}
                public Object get(long n,TimeUnit u){return null;}
                public long getDelay(TimeUnit u){return 0;}
                public int compareTo(Delayed other){return 0;}
            };
        }
    }
    public static void main(String[] args)throws Exception{
        Timer timer=new Timer();AtomicInteger closed=new AtomicInteger();
        try{
            BluetoothConnectDeadline complete=new BluetoothConnectDeadline(timer,closed::incrementAndGet);
            check(timer.delay==20000,"Fire 10.314 s connection must fit below the deadline");
            // Simulate an already queued timer callback after successful connect.
            complete.close();timer.callback.run();check(closed.get()==0,"stale callback closed completed connection");
            BluetoothConnectDeadline expired=new BluetoothConnectDeadline(timer,closed::incrementAndGet);
            timer.callback.run();timer.callback.run();expired.close();check(closed.get()==1,"timeout must close once");
            BluetoothConnectDeadline cancelled=new BluetoothConnectDeadline(timer,closed::incrementAndGet);
            cancelled.close();cancelled.close();timer.callback.run();check(closed.get()==1,"cancelled deadline fired");
            System.out.println("BluetoothConnectDeadlineTest: 4 checks passed");
        }finally{timer.shutdownNow();}
    }
}
