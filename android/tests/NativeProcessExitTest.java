// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** A force request is deliberately asynchronous, like Process.destroyForcibly(). */
public final class NativeProcessExitTest {
    static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    static final class DelayedProcess extends Process {
        final CountDownLatch killed=new CountDownLatch(1),dead=new CountDownLatch(1);
        volatile boolean alive=true;final int exit;
        DelayedProcess(int exit){this.exit=exit;}
        public OutputStream getOutputStream(){return new ByteArrayOutputStream();}
        public InputStream getInputStream(){return new ByteArrayInputStream(new byte[0]);}
        public InputStream getErrorStream(){return getInputStream();}
        public int waitFor()throws InterruptedException{dead.await();return exit;}
        public boolean waitFor(long timeout,TimeUnit unit)throws InterruptedException{return dead.await(timeout,unit);}
        public int exitValue(){if(alive)throw new IllegalThreadStateException();return exit;}
        public boolean isAlive(){return alive;}
        public void destroy(){destroyForcibly();}
        public Process destroyForcibly(){killed.countDown();return this;}
        void die(){alive=false;dead.countDown();}
    }
    public static void main(String[] args)throws Exception{
        DelayedProcess child=new DelayedProcess(137);
        FirmwareGate.Lease lease=FirmwareGate.use();AtomicBoolean stopped=new AtomicBoolean(),interrupted=new AtomicBoolean();
        Thread owner=new Thread(()->{NativeProcessExit.Result result=NativeProcessExit.await(child,10);check(result.forced&&result.exitCode==137,"Forced result lost");interrupted.set(Thread.currentThread().isInterrupted());lease.close();stopped.set(true);});
        owner.start();check(child.killed.await(2,TimeUnit.SECONDS),"Missing bounded force request");
        owner.interrupt();Thread.sleep(40);
        check(child.isAlive()&&!stopped.get()&&FirmwareGate.sessionActive(),"Asynchronous kill released ownership");
        try{FirmwareGate.change();throw new AssertionError("Live worker allowed card import");}catch(IOException expected){}
        child.die();owner.join(2000);check(!owner.isAlive()&&stopped.get()&&!FirmwareGate.sessionActive()&&interrupted.get(),"Death barrier or interrupt preservation failed");
        DelayedProcess normal=new DelayedProcess(0);normal.die();NativeProcessExit.Result ended=NativeProcessExit.await(normal,100);
        check(!ended.forced&&ended.exitCode==0&&normal.killed.getCount()==1,"Clean exit was forced");
        ChipsoftNativeStopGate gate=new ChipsoftNativeStopGate();check(gate.begin(100)&&!gate.begin(101),"Stop not once-only");
        check(!gate.cleanupAllowed("0F000000",101)&&gate.rejectQueued(101),"Diagnostic write accepted after Stop");
        for(String wire:new String[]{"050004000000880008800000","050004000000050005000000","2000000000000000"}){
            check(gate.cleanupAllowed(wire,101)&&!gate.cleanupAllowed(wire,102),"Cleanup missing or replayed");
        }
        gate.quitReceived();check(!gate.awaitingQuit()&&!gate.rejectQueued(102),"QUIT did not close Stop gate");
        gate.reset();gate.begin(100);check(gate.expired(100+ChipsoftNativeStopGate.GRACE_MS)&&!gate.cleanupAllowed("2000000000000000",100+ChipsoftNativeStopGate.GRACE_MS),"Deadline failed open");
        String failure=NativeSessionEnd.message("Chipsoft",5,false,true,"succeeded","failed");
        check(failure.contains("failed (exit 5)")&&failure.contains("Changes not verified")&&failure.contains("USB confirmed")&&failure.contains("Protocol cleanup failed"),"Stop hid native failure or conflated cleanup");
        String stoppedMessage=NativeSessionEnd.message("Nano",0,false,true,"succeeded","unknown");
        check(stoppedMessage.contains("session stopped")&&stoppedMessage.contains("Protocol cleanup unconfirmed")&&!stoppedMessage.contains("completed"),"Stop qualified workflow or invented cleanup");
        System.out.println("PASS: async force-death/image-lease barrier, interrupted owner, clean exit, bounded Chipsoft cleanup and no diagnostic replay");
    }
}
